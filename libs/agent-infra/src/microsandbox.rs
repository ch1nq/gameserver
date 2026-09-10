//! microsandbox [`MachineProvider`]: every machine is a real microVM with its
//! own guest kernel.
//!
//! Each sandbox gets its own isolated network with a host-side gateway, so all
//! match traffic relays through published ports on the host:
//!
//! ```text
//! coordinator (host process)
//!     |  127.0.0.1:{base+0}
//!     v
//! game host sandbox (`{match}-host`)
//!     |  host.microsandbox.internal:{base+n}
//!     v
//! agent n sandbox (`{match}-agent-{n}`, 0-based)
//! ```
//!
//! The host gets DNS plus host access narrowed to the agent relay ports. Agent
//! sandboxes get deny-by-default egress with no rules.
//!
//! # Load-bearing details
//!
//! - **`create()` does not run the image workload.** Call
//!   `exec_default_stream()` to start it, and do not await it.
//! - **Ingress must stay `Allow`.** Denying it closes the published port.
//! - **Sandboxes are detached**, so a coordinator crash leaves them for the
//!   reaper.
//! - **Requires KVM.**

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use microsandbox::sandbox::PullPolicy;
use microsandbox::{
    ExecEvent, MicrosandboxError, NetworkAction, NetworkPolicy, NetworkRule, RegistryAuth, Sandbox,
};

use crate::{
    AgentSlot, AgentSpawnConfig, ContainerImage, HostSpawnConfig, MachineError, MachineHandle,
    MachineProvider, MatchLayout, OrphanKind, OrphanedResource,
};

/// Label carrying the owning match id, for grouping and diagnostics.
const MATCH_LABEL: &str = "achtung.match";
/// Marker label identifying sandboxes this provider owns, for orphan scans.
const MANAGED_LABEL: &str = "achtung.managed";
const MANAGED_VALUE: &str = "1";

/// Username presented to the registry for private pulls, paired with a scoped
/// deploy JWT as the Basic password. `RegistryAuth` has no bearer variant.
const REGISTRY_SYSTEM_USER: &str = "system";

/// Page size for orphan scans. The SDK rejects a `limit` above 100.
const LIST_PAGE_SIZE: u32 = 100;

/// Guest-side hostname that resolves to the sandbox's host gateway. Agents are
/// reached by the game host through the host relay, not directly.
const HOST_INTERNAL: &str = "host.microsandbox.internal";

/// Prefix shared by every sandbox this provider creates. Also the reaper's
/// default match prefix.
const NAME_PREFIX: &str = "achtung-";

/// Configuration for the microsandbox machine provider.
/// Deserializable (via its [`Default`]) so the website config can build it
/// straight from a `[coordinator.microsandbox]` section, filling any omitted
/// field from the default.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct MicrosandboxMachineProviderConfig {
    /// vCPU limit per sandbox.
    pub cpus: u8,
    /// Memory limit per sandbox, in MiB.
    pub memory_mib: u32,
    /// First host port of the relay range. Slot `n` publishes on
    /// `host_port_base + n`.
    ///
    /// There is no allocator: one match runs at a time. A leftover sandbox
    /// holding a port is cleared by the pre-flight sweep in `init_match`.
    pub host_port_base: u16,
    /// Host address the *agent* relay ports bind to.
    ///
    /// `127.0.0.1` is correct if the guest netstack forwards a guest-originated
    /// connection to a loopback-bound published port. If it does not, the game
    /// host cannot reach agents and this becomes `0.0.0.0` — hence a knob rather
    /// than a constant. Slot 0 always binds loopback: its consumer is the
    /// coordinator, a host process, so widening it would add exposure for no
    /// gain.
    pub host_bind: IpAddr,
    /// Registry host prefixed onto private image refs.
    pub registry_pull_host: String,
    /// Pull over plain HTTP. Local dev only — the deploy JWT crosses the wire in
    /// cleartext.
    pub registry_insecure: bool,
    /// Hard lifetime cap per sandbox, host-enforced (the guest cannot override
    /// it). Backstop for a match that never reports completion.
    pub max_duration_secs: Option<u64>,
}

impl Default for MicrosandboxMachineProviderConfig {
    fn default() -> Self {
        Self {
            cpus: 1,
            memory_mib: 512,
            host_port_base: 51000,
            host_bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            registry_pull_host: "localhost:5001".to_string(),
            registry_insecure: false,
            max_duration_secs: None,
        }
    }
}

/// Per-match context. The layout is retained because the host's egress policy
/// depends on the full relay port range, which is only known up front.
pub struct MicrosandboxMatchContext {
    match_id: MatchId,
    layout: MatchLayout,
}

impl MicrosandboxMatchContext {
    /// Validated agent slots for this match.
    pub fn layout(&self) -> MatchLayout {
        self.layout
    }
}

/// Match identifier, distinct from sandbox and image names.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MatchId(String);

impl MatchId {
    fn new(match_id: &str) -> Self {
        Self(match_id.to_string())
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MatchId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Image ref microsandbox should pull, plus the deploy token for private images.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedImage {
    reference: String,
    token: Option<String>,
}

/// microsandbox implementation of [`MachineProvider`].
pub struct MicrosandboxMachineProvider {
    config: MicrosandboxMachineProviderConfig,
}

impl MicrosandboxMachineProvider {
    pub fn new(config: MicrosandboxMachineProviderConfig) -> Self {
        Self { config }
    }

    /// Host relay port for the game host (raw slot 0).
    fn host_port_for_host(&self) -> u16 {
        self.config.host_port_base
    }

    /// Host relay port for an agent (raw slot `index + 1`).
    ///
    /// Cannot overflow: `init_match` rejects layouts whose relay range exceeds
    /// `u16`, so the plain add here is safe.
    fn host_port_for_agent(&self, slot: AgentSlot) -> u16 {
        self.config.host_port_base + u16::from(slot.raw_slot())
    }

    /// Resolve a [`ContainerImage`] to the ref microsandbox should pull, plus the
    /// deploy token when the image is private.
    fn image_ref(&self, image: &ContainerImage) -> ResolvedImage {
        match image {
            // Public/local: used verbatim, so a locally `msb load`-ed tag works.
            ContainerImage::Public(url) => ResolvedImage {
                reference: url.as_ref().to_string(),
                token: None,
            },
            ContainerImage::Private {
                image_url,
                registry_token,
            } => ResolvedImage {
                reference: format!("{}/{}", self.config.registry_pull_host, image_url.as_ref()),
                token: Some(registry_token.as_ref().to_string()),
            },
        }
    }

    /// Egress policy for the game host (trusted image).
    ///
    /// Ingress stays `Allow`: it admits traffic on the published port, and
    /// `default_deny()` would close it.
    ///
    /// Gets DNS plus host access **narrowed to the agent relay ports**, so it
    /// cannot reach Postgres, the registry, or `/registry/token` on the host.
    /// The range is always non-empty — zero-agent matches are rejected when
    /// the [`MatchLayout`] is built.
    fn policy_for_host(&self, layout: MatchLayout) -> Result<NetworkPolicy, MachineError> {
        let range = layout.relay_range(self.config.host_port_base)?;
        let builder = NetworkPolicy::builder()
            .default_egress(NetworkAction::Deny)
            .default_ingress(NetworkAction::Allow)
            .egress(|e| {
                e.tcp()
                    .port_range(*range.start(), *range.end())
                    .allow_host()
            });

        let mut policy = builder.build().map_err(|e| {
            MachineError::MachineCreation(format!("build network policy for game host: {e}"))
        })?;

        // DNS prepended as a prebuilt rule rather than composed in the builder:
        // under deny-by-default a query has no resolved IP yet, so only the
        // gateway-forwarder `Host` group can match it, and `allow_dns()` is
        // the SDK's canonical encoding of exactly that.
        policy.rules.insert(0, NetworkRule::allow_dns());

        Ok(policy)
    }

    /// Egress policy for agents (untrusted images): deny-by-default with zero
    /// rules. Blocks the internet, the host, and — critically — the relay
    /// ports fronting sibling agents. Structural isolation: with an empty rule
    /// list there is no rule to misconfigure.
    fn policy_for_agent(&self) -> Result<NetworkPolicy, MachineError> {
        NetworkPolicy::builder()
            .default_egress(NetworkAction::Deny)
            .default_ingress(NetworkAction::Allow)
            .build()
            .map_err(|e| {
                MachineError::MachineCreation(format!("build network policy for agent: {e}"))
            })
    }

    /// Whether an error means "this sandbox is already gone", so destroy paths
    /// can be idempotent.
    fn is_gone(err: &MicrosandboxError) -> bool {
        matches!(err, MicrosandboxError::SandboxNotFound(_))
    }

    /// Stop and remove a sandbox by name, tolerating one that is already gone or
    /// already stopped.
    async fn destroy_by_name(&self, name: &str) -> Result<(), MachineError> {
        let handle = match Sandbox::get(name).await {
            Ok(handle) => handle,
            Err(e) if Self::is_gone(&e) => return Ok(()),
            Err(e) => return Err(MachineError::Destruction(format!("get {name}: {e}"))),
        };

        // A crashed or already-stopped sandbox still needs removing, so a
        // not-running stop is not an error here.
        match handle.stop().await {
            Ok(()) => {}
            Err(MicrosandboxError::SandboxNotRunning(_)) => {}
            Err(e) if Self::is_gone(&e) => return Ok(()),
            Err(e) => tracing::warn!(name, error = %e, "Stop failed; attempting remove anyway"),
        }

        match handle.remove().await {
            Ok(()) => Ok(()),
            Err(e) if Self::is_gone(&e) => Ok(()),
            Err(e) => Err(MachineError::Destruction(format!("remove {name}: {e}"))),
        }
    }
}

/// How a sandbox's guest port is published on the host.
enum PortPublish {
    /// Game host: consumed by the coordinator on the host, so loopback always
    /// suffices and wider exposure adds nothing.
    HostLoopback,
    /// Agent: consumed by the game host from inside a guest; may need a wider
    /// bind to be reachable — see `host_bind`.
    AgentRelay(IpAddr),
}

/// Per-machine inputs to `spawn_sandbox`.
///
/// Bundled so the shared spawn body stays under clippy's too-many-arguments
/// lint. Role-specific values are resolved by `spawn_host` / `spawn_agent`
/// before calling.
struct SandboxSpawn<'a> {
    name: MachineName,
    host_port: u16,
    guest_port: u16,
    container_image: &'a ContainerImage,
    env: &'a std::collections::HashMap<String, String>,
    policy: NetworkPolicy,
    publish: PortPublish,
    private_ip: String,
    agent_slot: Option<AgentSlot>,
}

impl MicrosandboxMachineProvider {
    /// Shared spawn body for host and agents. Role-specific inputs (name,
    /// ports, policy, publish mode, address) are resolved by the caller, so
    /// this function never branches on a role value.
    async fn spawn_sandbox(
        &self,
        ctx: &MicrosandboxMatchContext,
        spawn: SandboxSpawn<'_>,
    ) -> Result<MachineHandle, MachineError> {
        let SandboxSpawn {
            name,
            host_port,
            guest_port,
            container_image,
            env,
            policy,
            publish,
            private_ip,
            agent_slot,
        } = spawn;
        let resolved = self.image_ref(container_image);
        let image = resolved.reference.clone();
        let token = resolved.token;

        let mut builder = Sandbox::builder(name.as_str())
            .image(image.clone())
            .pull_policy(PullPolicy::IfMissing)
            .cpus(self.config.cpus)
            .memory(self.config.memory_mib)
            .label(MANAGED_LABEL, MANAGED_VALUE)
            .label(MATCH_LABEL, ctx.match_id.as_str())
            .network(|n| n.policy(policy))
            // Survive a coordinator crash so the reaper can collect them, rather
            // than dying with a dropped in-process handle.
            .detached(true)
            // A leftover sandbox of the same name would fail the create
            // outright; the pre-flight sweep makes that rare, not impossible.
            .replace();

        // A single `registry()` call: the builder assigns `insecure` wholesale,
        // so a second call would clobber the first.
        let insecure = self.config.registry_insecure;
        builder = builder.registry(move |r| {
            let r = match token {
                Some(password) => r.auth(RegistryAuth::Basic {
                    username: REGISTRY_SYSTEM_USER.to_string(),
                    password,
                }),
                None => r,
            };
            if insecure { r.insecure() } else { r }
        });

        builder = match publish {
            PortPublish::HostLoopback => builder.port(host_port, guest_port),
            PortPublish::AgentRelay(bind) => builder.port_bind(bind, host_port, guest_port),
        };

        for (key, value) in env {
            builder = builder.env(key, value);
        }

        if let Some(secs) = self.config.max_duration_secs {
            builder = builder.max_duration(secs);
        }

        // Separate the pull failure from the boot failure: the first is a
        // registry/auth problem, the second a host or image problem.
        let sandbox = builder.create().await.map_err(|e| {
            if matches!(
                e,
                MicrosandboxError::Image(_) | MicrosandboxError::ImageNotFound(_)
            ) {
                MachineError::ImageCopy(format!("pull {image} for {name}: {e}"))
            } else {
                MachineError::MachineCreation(format!("create {name}: {e}"))
            }
        })?;

        // Creation is boot-only, so nothing is running yet. Start the image's
        // effective ENTRYPOINT + CMD and do NOT await it: it runs for the whole
        // match. `exec_default` (non-streaming) would block here, and the
        // coordinator would time out dialing a machine never actually started.
        let exec = sandbox.exec_default_stream().await.map_err(|e| {
            MachineError::MachineCreation(match &e {
                MicrosandboxError::NoDefaultCommand => format!(
                    "image {image} has no ENTRYPOINT or CMD, so there is no workload to start"
                ),
                _ => format!("start default workload in {name}: {e}"),
            })
        })?;
        drain_workload_output(exec, name.clone());

        // Release the handle without stopping the VM. Consumes `sandbox`, so
        // this must come after the exec above.
        sandbox.detach().await;

        tracing::info!(
            match_id = %ctx.match_id,
            sandbox = %name,
            image,
            agent_slot = agent_slot.map(|s| s.index()),
            private_ip,
            host_port,
            guest_port,
            "Spawned microsandbox microVM"
        );

        Ok(MachineHandle {
            app_name: ctx.match_id.as_str().to_string(),
            machine_id: name.as_str().to_string(),
            private_ip,
            grpc_port: Some(host_port),
        })
    }

    /// Clear sandboxes left by a previous run before starting a match.
    ///
    /// Relay ports are fixed (`base + slot`), so a leftover sandbox holds the
    /// port this match needs and `create()` would fail. `Duration::ZERO` makes
    /// every one of our sandboxes eligible, which is correct because only one
    /// match runs at a time.
    async fn sweep_stale_sandboxes(&self, match_id: &MatchId) {
        match self.list_orphaned(NAME_PREFIX, Duration::ZERO).await {
            Ok(stale) if !stale.is_empty() => {
                tracing::warn!(
                    count = stale.len(),
                    match_id = %match_id,
                    "Clearing sandboxes left by a previous run before starting match"
                );
                for resource in &stale {
                    if let Err(e) = self.destroy_orphaned(resource).await {
                        // Not fatal on its own: only a name or port collision
                        // actually blocks us, and `create()` reports that.
                        tracing::warn!(name = %resource.name, error = %e, "Pre-flight sweep failed");
                    }
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "Pre-flight sweep could not list sandboxes"),
        }
    }
}

/// Sandbox name for the game host. Also the reaper's match key,
/// so it must carry [`NAME_PREFIX`].
fn host_sandbox_name(match_id: &str) -> MachineName {
    MachineName(format!("{NAME_PREFIX}{match_id}-host"))
}

/// Sandbox name for an agent (0-based index). Also the reaper's match key.
fn agent_sandbox_name(match_id: &str, slot: AgentSlot) -> MachineName {
    MachineName(format!("{NAME_PREFIX}{match_id}-agent-{}", slot.index()))
}

/// Sandbox name, distinct from match ids and image refs.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MachineName(String);

impl MachineName {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MachineName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Ensure the `msb` runtime and `libkrunfw` are present, downloading them to
/// `~/.microsandbox` if not.
///
/// Call this before constructing a provider so a missing runtime fails at
/// startup: without it every spawn fails, and the first symptom would be a
/// match that never starts. Idempotent — a matching install is reused.
///
/// Exposed here so callers need not depend on the `microsandbox` crate directly.
pub async fn ensure_runtime_installed() -> Result<(), MachineError> {
    if microsandbox::setup::is_installed() {
        return Ok(());
    }
    tracing::warn!("microsandbox runtime missing; installing to ~/.microsandbox");
    microsandbox::setup::install()
        .await
        .map_err(|e| MachineError::MatchInit(format!("install microsandbox runtime: {e}")))
}

/// Forward a workload's output into `tracing` until the process exits.
///
/// Draining rather than dropping the handle: whether a dropped `ExecHandle`
/// signals the guest process is undocumented, and this is the only window into a
/// workload that dies during startup.
fn drain_workload_output(mut exec: microsandbox::ExecHandle, machine: MachineName) {
    tokio::spawn(async move {
        while let Some(event) = exec.recv().await {
            match event {
                ExecEvent::Started { pid } => {
                    tracing::debug!(machine = %machine, pid, "Workload started");
                }
                ExecEvent::Stdout(chunk) => {
                    tracing::debug!(
                        machine = %machine,
                        "{}",
                        String::from_utf8_lossy(&chunk).trim_end()
                    );
                }
                ExecEvent::Stderr(chunk) => {
                    tracing::warn!(
                        machine = %machine,
                        "{}",
                        String::from_utf8_lossy(&chunk).trim_end()
                    );
                }
                ExecEvent::Exited { code } => {
                    // Expected at teardown; mid-match this is the first sign of
                    // why the machine stopped answering.
                    tracing::info!(machine = %machine, code, "Workload exited");
                }
                ExecEvent::Failed(failed) => {
                    tracing::error!(
                        machine = %machine,
                        kind = ?failed.kind,
                        "Workload failed to spawn: {}",
                        failed.message
                    );
                }
                ExecEvent::StdinError(err) => {
                    tracing::warn!(
                        machine = %machine,
                        errno = ?err.errno,
                        "Workload stdin write failed: {}",
                        err.message
                    );
                }
            }
        }
    });
}

#[async_trait::async_trait]
impl MachineProvider for MicrosandboxMachineProvider {
    type MatchContext = MicrosandboxMatchContext;

    async fn init_match(
        &self,
        match_id: &str,
        layout: MatchLayout,
    ) -> Result<MicrosandboxMatchContext, MachineError> {
        // Fail fast on port overflow: relay ports are fixed (`base + slot`),
        // so an overflowing range would silently collide at spawn time.
        layout.relay_range(self.config.host_port_base)?;
        let ctx = MicrosandboxMatchContext {
            match_id: MatchId::new(match_id),
            layout,
        };
        self.sweep_stale_sandboxes(&ctx.match_id).await;
        Ok(ctx)
    }

    async fn spawn_host(
        &self,
        ctx: &MicrosandboxMatchContext,
        config: HostSpawnConfig,
    ) -> Result<MachineHandle, MachineError> {
        let name = host_sandbox_name(ctx.match_id.as_str());
        let host_port = self.host_port_for_host();
        let policy = self.policy_for_host(ctx.layout)?;
        self.spawn_sandbox(
            ctx,
            SandboxSpawn {
                name,
                host_port,
                guest_port: config.grpc_port,
                container_image: &config.container_image,
                env: &config.env,
                policy,
                publish: PortPublish::HostLoopback,
                // Consumer-relative addressing: the coordinator reads the host
                // from the host itself.
                private_ip: Ipv4Addr::LOCALHOST.to_string(),
                agent_slot: None,
            },
        )
        .await
    }

    async fn spawn_agent(
        &self,
        ctx: &MicrosandboxMatchContext,
        config: AgentSpawnConfig,
    ) -> Result<MachineHandle, MachineError> {
        // Defense-in-depth: the coordinator can only build slots from this
        // match's layout, so this never fires unless callers mix matches.
        if config.slot.index() >= ctx.layout.num_agents() {
            return Err(MachineError::MachineCreation(format!(
                "agent slot {} out of range for {} agents",
                config.slot.index(),
                ctx.layout.num_agents()
            )));
        }
        let name = agent_sandbox_name(ctx.match_id.as_str(), config.slot);
        let host_port = self.host_port_for_agent(config.slot);
        let policy = self.policy_for_agent()?;
        self.spawn_sandbox(
            ctx,
            SandboxSpawn {
                name,
                host_port,
                guest_port: config.grpc_port,
                container_image: &config.container_image,
                env: &config.env,
                policy,
                publish: PortPublish::AgentRelay(self.config.host_bind),
                // The game host reads agents from inside a guest, via the host relay.
                private_ip: HOST_INTERNAL.to_string(),
                agent_slot: Some(config.slot),
            },
        )
        .await
    }

    async fn destroy(
        &self,
        _ctx: &MicrosandboxMatchContext,
        handle: &MachineHandle,
    ) -> Result<(), MachineError> {
        self.destroy_by_name(&handle.machine_id).await
    }

    async fn cleanup_match(&self, ctx: MicrosandboxMatchContext) -> Result<(), MachineError> {
        // Nothing shared to release: each sandbox owns its own /30 and gateway,
        // created and torn down with the VM, and published host ports are freed
        // when `destroy` removes the sandbox.
        tracing::debug!(
            match_id = %ctx.match_id,
            "microsandbox match cleanup: no shared resources"
        );
        Ok(())
    }

    async fn list_orphaned(
        &self,
        prefix: &str,
        max_age: Duration,
    ) -> Result<Vec<OrphanedResource>, MachineError> {
        let now = SystemTime::now();
        let mut orphaned = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            // Filter on the constant marker label, then narrow in Rust:
            // `SandboxHandle` exposes no label accessor, so the match id cannot
            // be read back off a listed sandbox.
            let page_cursor = cursor.take();
            let page = Sandbox::list_with(move |list| {
                let list = list
                    .limit(LIST_PAGE_SIZE)
                    .label(MANAGED_LABEL, MANAGED_VALUE);
                match page_cursor {
                    Some(c) => list.cursor(c),
                    None => list,
                }
            })
            .await
            .map_err(|e| MachineError::Destruction(format!("list sandboxes: {e}")))?;

            for handle in &page.sandboxes {
                let name = handle.name().to_string();
                if !name.starts_with(prefix) {
                    continue;
                }

                // No recorded creation time means we cannot prove it is old
                // enough. Treating it as brand new errs toward leaving a live
                // match alone rather than killing it mid-game.
                let created_at = handle
                    .created_at()
                    .map(|ts| UNIX_EPOCH + Duration::from_secs(ts.timestamp().max(0) as u64))
                    .unwrap_or(now);

                if now.duration_since(created_at).unwrap_or(Duration::ZERO) >= max_age {
                    orphaned.push(OrphanedResource {
                        // microsandbox addresses sandboxes by name, so id == name.
                        id: name.clone(),
                        name,
                        created_at,
                        // This backend creates no networks: each sandbox's /30
                        // lives and dies with its VM.
                        kind: OrphanKind::Machine,
                    });
                }
            }

            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }

        tracing::info!(
            count = orphaned.len(),
            prefix,
            "microsandbox orphan scan complete"
        );
        Ok(orphaned)
    }

    async fn destroy_orphaned(&self, resource: &OrphanedResource) -> Result<(), MachineError> {
        self.destroy_by_name(&resource.id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> MicrosandboxMachineProvider {
        MicrosandboxMachineProvider::new(MicrosandboxMachineProviderConfig {
            host_port_base: 51000,
            registry_pull_host: "registry:5001".to_string(),
            ..Default::default()
        })
    }

    #[test]
    fn sandbox_names_carry_the_reaper_prefix() {
        let host = host_sandbox_name("abc123");
        assert_eq!(host.as_str(), "achtung-abc123-host");
        let agent = agent_sandbox_name("abc123", AgentSlot::from_index(1).unwrap());
        assert_eq!(agent.as_str(), "achtung-abc123-agent-1");
        // The reaper filters on this prefix; renaming here silently stops
        // orphan collection.
        assert!(host.as_str().starts_with(NAME_PREFIX));
        assert!(agent.as_str().starts_with(NAME_PREFIX));
    }

    #[test]
    fn host_ports_are_slot_offsets_from_the_base() {
        let p = provider();
        assert_eq!(p.host_port_for_host(), 51000);
        assert_eq!(
            p.host_port_for_agent(AgentSlot::from_index(2).unwrap()),
            51003
        );
    }

    #[test]
    fn public_images_are_used_verbatim_with_no_credentials() {
        let p = provider();
        let image = ContainerImage::Public(common::ImageUrl::from(
            "achtung-game-host:local".to_string(),
        ));
        let resolved = p.image_ref(&image);
        assert_eq!(resolved.reference, "achtung-game-host:local");
        assert!(
            resolved.token.is_none(),
            "public pulls must not present credentials"
        );
    }

    #[test]
    fn private_images_are_prefixed_and_carry_the_token() {
        let p = provider();
        let image = ContainerImage::Private {
            image_url: common::ImageUrl::from("user-5/bot:v1".to_string()),
            registry_token: common::RegistryToken::from("jwt-value".to_string()),
        };
        let resolved = p.image_ref(&image);
        assert_eq!(resolved.reference, "registry:5001/user-5/bot:v1");
        assert_eq!(resolved.token.as_deref(), Some("jwt-value"));
    }

    #[test]
    fn host_and_agents_deny_egress_but_permit_ingress() {
        let p = provider();
        let layout = MatchLayout::new(2).unwrap();
        for policy in [
            p.policy_for_host(layout).expect("host policy builds"),
            p.policy_for_agent().expect("agent policy builds"),
        ] {
            assert_eq!(policy.default_egress, NetworkAction::Deny);
            // Ingress Allow is what admits the published port. `default_deny()`
            // would set both directions and silently break the relay.
            assert_eq!(policy.default_ingress, NetworkAction::Allow);
        }
    }

    #[test]
    fn agents_get_no_egress_rules_at_all() {
        let p = provider();
        // Structural isolation: with deny-by-default and an empty rule list
        // there is no rule to misconfigure, so an agent cannot reach the
        // internet, the host, or the relay ports fronting sibling agents.
        let policy = p.policy_for_agent().expect("policy builds");
        assert!(
            policy.rules.is_empty(),
            "agents must have zero egress rules, found {:?}",
            policy.rules
        );
    }

    #[test]
    fn game_host_reaches_only_dns_and_the_agent_relay_range() {
        let p = provider();
        let layout = MatchLayout::new(3).unwrap();
        let policy = p.policy_for_host(layout).expect("policy builds");

        // DNS (53) plus the relay range, and nothing else.
        assert_eq!(
            policy.rules.len(),
            2,
            "unexpected rules: {:?}",
            policy.rules
        );

        let relay = policy
            .rules
            .iter()
            .find(|r| r.ports.iter().all(|p| p.start != 53))
            .expect("a relay rule exists");
        let range = relay
            .ports
            .first()
            .expect("the relay rule is port-scoped, not any-port");

        // Exactly slots 1..=3 — never slot 0's own port, and never a wider range
        // that would expose Postgres, the registry, or /registry/token.
        assert_eq!((range.start, range.end), (51001, 51003));
    }
}
