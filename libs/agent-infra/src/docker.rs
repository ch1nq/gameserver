//! Docker [`MachineProvider`] with two isolation modes.
//!
//! # [`DockerIsolation::SharedNetwork`] (local dev)
//!
//! Runs the game host and agent containers on a shared user-defined network so
//! the coordinator (running in the website container) can reach them by
//! container name via Docker's embedded DNS. Each container is named
//! `achtung-{match}-slot-{n}` and that name is used as its `private_ip`, so the
//! coordinator dials `http://{name}:50051` (game host) / `{name}:50052`
//! (agents), resolved on the shared network. No sandboxing beyond runc.
//!
//! # [`DockerIsolation::PerMatchNetworks`] (production, gVisor)
//!
//! Runs every container under a hardened runtime (`runsc`) and derives network
//! isolation from *topology* instead of firewall rules:
//!
//! - `init_match` creates one **internal** Docker network per slot
//!   (`achtung-{match}-s{n}`). Internal networks have no NAT and no default
//!   route, so guests have no internet access.
//! - Each agent lives alone on its slot network; the game host (slot 0) is
//!   connected to *every* slot network **before it starts** — gVisor cannot
//!   attach networks to a running sandbox, so the attach order is load-bearing.
//! - Agents therefore share no L2 segment with each other. Cross-network
//!   traffic is routed, traverses FORWARD, and is dropped by Docker's own
//!   isolation chains — no `br_netfilter`, fails closed.
//! - Addressing is by IP (inspected after start), not container name: the
//!   coordinator runs on the host, outside Docker DNS, and gVisor's netstack
//!   is not relied on for the embedded resolver.
//! - Agents get cgroup limits, a read-only rootfs, and sized tmpfs mounts so a
//!   hostile image cannot exhaust host disk. The game host (trusted image)
//!   keeps a writable rootfs but the same CPU/memory limits.
//!
//! Guest→host traffic is *not* blocked here — that is a static, install-time
//! iptables INPUT policy (see `scripts/setup-gvisor/`).
//!
//! Images: the game host is a public/local image (pulled only if absent, so a
//! locally-built tag works); agent images are pulled from the private registry
//! using the coordinator's scoped deploy token as a bearer credential.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bollard::Docker;
use bollard::auth::DockerCredentials;
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use bollard::network::{ConnectNetworkOptions, CreateNetworkOptions, ListNetworksOptions};
use futures_util::StreamExt;

use crate::{
    ContainerImage, MachineError, MachineHandle, MachineProvider, OrphanKind, OrphanedResource,
    SpawnConfig,
};

/// Label carrying the owning match id, set on per-match networks.
const MATCH_LABEL: &str = "achtung.match";
/// Label carrying the unix creation time, used by the orphan reaper.
const CREATED_AT_LABEL: &str = "achtung.created_at";

/// tmpfs mounts given to agent containers alongside the read-only rootfs.
/// Sized so a hostile image cannot exhaust host disk or dodge its memory cap.
const AGENT_TMPFS: &[(&str, &str)] = &[("/tmp", "rw,size=64m"), ("/run", "rw,size=16m")];

/// How match containers are isolated from each other and the outside world.
#[derive(Debug, Clone)]
pub enum DockerIsolation {
    /// Local dev: all containers on one pre-existing network, name-based
    /// addressing, default runtime. Provides **no** isolation between agents.
    SharedNetwork {
        /// Docker network to attach match containers to. Must be the same
        /// network the website/coordinator container is on so it can resolve
        /// them by name.
        network: String,
    },
    /// Production: per-slot internal networks + a sandboxing runtime.
    PerMatchNetworks {
        /// Container runtime, e.g. `"runsc"` (gVisor). `"runc"` is useful to
        /// test the network topology without gVisor installed.
        runtime: String,
        /// CPU limit per container, in units of 1e-9 CPUs (1 CPU = 1_000_000_000).
        nano_cpus: i64,
        /// Memory limit per container, in bytes.
        memory_bytes: i64,
        /// Max processes per container (fork-bomb containment).
        pids_limit: i64,
    },
}

/// Configuration for the Docker machine provider.
#[derive(Debug, Clone)]
pub struct DockerMachineProviderConfig {
    /// Isolation mode; see [`DockerIsolation`].
    pub isolation: DockerIsolation,
    /// Registry host used to build private image pull refs, reachable by the
    /// Docker daemon (e.g. `localhost:5001`, which the daemon treats as
    /// insecure automatically).
    pub registry_pull_host: String,
}

/// Per-match context: the match id plus (in per-match-network mode) the slot
/// networks created by `init_match`, indexed by slot.
pub struct DockerMatchContext {
    match_id: String,
    /// Slot networks (`networks[n]` belongs to slot n). Empty in shared mode.
    networks: Vec<String>,
}

/// Local Docker implementation of [`MachineProvider`].
pub struct DockerMachineProvider {
    docker: Docker,
    config: DockerMachineProviderConfig,
}

impl DockerMachineProvider {
    /// Connect to the local Docker daemon over its default unix socket.
    pub fn new(config: DockerMachineProviderConfig) -> Result<Self, MachineError> {
        let docker = Docker::connect_with_unix_defaults()
            .map_err(|e| MachineError::MatchInit(format!("connect to docker daemon: {e}")))?;
        Ok(Self { docker, config })
    }

    /// Split an image ref into `(name, tag)` for the pull API. Handles the
    /// registry-port colon (e.g. `localhost:5001/x/y:tag`) by only treating the
    /// last `:` as a tag separator when it has no `/` after it.
    fn split_ref(image: &str) -> (String, String) {
        match image.rsplit_once(':') {
            Some((name, tag)) if !tag.contains('/') => (name.to_string(), tag.to_string()),
            _ => (image.to_string(), "latest".to_string()),
        }
    }

    async fn pull(
        &self,
        image: &str,
        credentials: Option<DockerCredentials>,
    ) -> Result<(), MachineError> {
        let (from_image, tag) = Self::split_ref(image);
        let options = CreateImageOptions {
            from_image,
            tag,
            ..Default::default()
        };
        let mut stream = self.docker.create_image(Some(options), None, credentials);
        while let Some(item) = stream.next().await {
            item.map_err(|e| MachineError::ImageCopy(format!("pull {image}: {e}")))?;
        }
        Ok(())
    }

    /// Resolve `config.container_image` to a locally-available image ref,
    /// pulling as needed. Returns the ref to run.
    async fn ensure_image(&self, image: &ContainerImage) -> Result<String, MachineError> {
        match image {
            // Public/local: pull only if absent, so a locally-built tag works.
            ContainerImage::Public(url) => {
                let reference = url.as_ref().to_string();
                if self.docker.inspect_image(&reference).await.is_err() {
                    self.pull(&reference, None).await?;
                }
                Ok(reference)
            }
            // Private agent: pull from the registry with the deploy token as a
            // bearer credential (X-Registry-Auth registrytoken).
            ContainerImage::Private {
                image_url,
                registry_token,
            } => {
                let reference =
                    format!("{}/{}", self.config.registry_pull_host, image_url.as_ref());
                let credentials = DockerCredentials {
                    registrytoken: Some(registry_token.as_ref().to_string()),
                    ..Default::default()
                };
                self.pull(&reference, Some(credentials)).await?;
                Ok(reference)
            }
        }
    }

    /// Whether a bollard error is a 404 (resource already gone).
    fn is_not_found(err: &bollard::errors::Error) -> bool {
        matches!(
            err,
            bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                ..
            }
        )
    }

    /// The container's IP address on the given network, after start.
    async fn container_ip(&self, name: &str, network: &str) -> Result<String, MachineError> {
        let details = self
            .docker
            .inspect_container(name, None)
            .await
            .map_err(|e| MachineError::IpAssignment(format!("inspect {name}: {e}")))?;

        details
            .network_settings
            .and_then(|ns| ns.networks)
            .and_then(|mut nets| nets.remove(network))
            .and_then(|ep| ep.ip_address)
            .filter(|ip| !ip.is_empty())
            .ok_or_else(|| {
                MachineError::IpAssignment(format!("no IP for {name} on network {network}"))
            })
    }

    /// The `HostConfig` for a slot under the configured isolation mode.
    ///
    /// In per-match mode, agents (slot != 0) additionally get a read-only
    /// rootfs with sized tmpfs mounts; the game host image is trusted and may
    /// need to write, so it keeps a writable rootfs.
    fn host_config(&self, ctx: &DockerMatchContext, slot: u8) -> HostConfig {
        match &self.config.isolation {
            DockerIsolation::SharedNetwork { network } => HostConfig {
                network_mode: Some(network.clone()),
                ..Default::default()
            },
            DockerIsolation::PerMatchNetworks {
                runtime,
                nano_cpus,
                memory_bytes,
                pids_limit,
            } => {
                let agent = slot != 0;
                HostConfig {
                    network_mode: Some(ctx.networks[slot as usize].clone()),
                    runtime: Some(runtime.clone()),
                    nano_cpus: Some(*nano_cpus),
                    memory: Some(*memory_bytes),
                    pids_limit: Some(*pids_limit),
                    readonly_rootfs: Some(agent),
                    tmpfs: agent.then(|| {
                        AGENT_TMPFS
                            .iter()
                            .map(|(path, opts)| (path.to_string(), opts.to_string()))
                            .collect()
                    }),
                    ..Default::default()
                }
            }
        }
    }

    /// Remove a network, tolerating 404 (already gone).
    async fn remove_network(&self, name: &str) -> Result<(), MachineError> {
        match self.docker.remove_network(name).await {
            Ok(()) => Ok(()),
            Err(e) if Self::is_not_found(&e) => Ok(()),
            Err(e) => Err(MachineError::Destruction(format!(
                "remove network {name}: {e}"
            ))),
        }
    }

    /// Per-match-mode networks labeled as ours and older than `max_age`.
    async fn list_orphaned_networks(
        &self,
        max_age: Duration,
    ) -> Result<Vec<OrphanedResource>, MachineError> {
        let mut filters = HashMap::new();
        filters.insert("label".to_string(), vec![MATCH_LABEL.to_string()]);
        let networks = self
            .docker
            .list_networks(Some(ListNetworksOptions { filters }))
            .await
            .map_err(|e| MachineError::Destruction(format!("list networks: {e}")))?;

        let now = SystemTime::now();
        let mut orphaned = Vec::new();
        for net in networks {
            let Some(name) = net.name else { continue };
            let created_at = net
                .labels
                .as_ref()
                .and_then(|l| l.get(CREATED_AT_LABEL))
                .and_then(|s| s.parse::<u64>().ok())
                .map(|secs| UNIX_EPOCH + Duration::from_secs(secs))
                .unwrap_or(now);
            if now.duration_since(created_at).unwrap_or(Duration::ZERO) >= max_age {
                orphaned.push(OrphanedResource {
                    id: net.id.unwrap_or_else(|| name.clone()),
                    name,
                    created_at,
                    kind: OrphanKind::Network,
                });
            }
        }
        Ok(orphaned)
    }
}

fn container_name(match_id: &str, slot: u8) -> String {
    format!("achtung-{match_id}-slot-{slot}")
}

fn network_name(match_id: &str, slot: u8) -> String {
    format!("achtung-{match_id}-s{slot}")
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[async_trait::async_trait]
impl MachineProvider for DockerMachineProvider {
    type MatchContext = DockerMatchContext;

    async fn init_match(
        &self,
        match_id: &str,
        num_slots: u8,
    ) -> Result<DockerMatchContext, MachineError> {
        let networks = match &self.config.isolation {
            // Containers share the pre-existing network; nothing to create.
            DockerIsolation::SharedNetwork { .. } => Vec::new(),
            DockerIsolation::PerMatchNetworks { .. } => {
                let created_at = unix_now_secs().to_string();
                let mut networks: Vec<String> = Vec::with_capacity(num_slots as usize);
                for slot in 0..num_slots {
                    let name = network_name(match_id, slot);
                    let options = CreateNetworkOptions {
                        name: name.clone(),
                        // No NAT, no default route: guests get no internet.
                        internal: true,
                        labels: HashMap::from([
                            (MATCH_LABEL.to_string(), match_id.to_string()),
                            (CREATED_AT_LABEL.to_string(), created_at.clone()),
                        ]),
                        ..Default::default()
                    };
                    if let Err(e) = self.docker.create_network(options).await {
                        // Roll back networks created so far; a half-initialized
                        // match would otherwise leak until the reaper runs.
                        for name in &networks {
                            let _ = self.remove_network(name).await;
                        }
                        return Err(MachineError::MatchInit(format!(
                            "create network {name}: {e}"
                        )));
                    }
                    networks.push(name);
                }
                tracing::info!(match_id, count = networks.len(), "Match networks created");
                networks
            }
        };

        Ok(DockerMatchContext {
            match_id: match_id.to_string(),
            networks,
        })
    }

    async fn spawn(
        &self,
        ctx: &DockerMatchContext,
        config: SpawnConfig,
    ) -> Result<MachineHandle, MachineError> {
        let slot = config.slot;
        let per_match = matches!(
            self.config.isolation,
            DockerIsolation::PerMatchNetworks { .. }
        );
        if per_match && slot as usize >= ctx.networks.len() {
            return Err(MachineError::MachineCreation(format!(
                "slot {slot} exceeds the {} slots allocated by init_match",
                ctx.networks.len()
            )));
        }

        let name = container_name(&ctx.match_id, slot);
        let image = self.ensure_image(&config.container_image).await?;

        let env: Vec<String> = config.env.iter().map(|(k, v)| format!("{k}={v}")).collect();

        let container_config = Config {
            image: Some(image.clone()),
            env: Some(env),
            host_config: Some(self.host_config(ctx, slot)),
            ..Default::default()
        };

        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: name.clone(),
                    platform: None,
                }),
                container_config,
            )
            .await
            .map_err(|e| MachineError::MachineCreation(format!("create {name}: {e}")))?;

        // The game host must reach every agent, so connect it to all the other
        // slot networks. This MUST happen before start: gVisor sandboxes cannot
        // have networks attached once running.
        if per_match && slot == 0 {
            for network in &ctx.networks[1..] {
                self.docker
                    .connect_network(
                        network,
                        ConnectNetworkOptions {
                            container: name.clone(),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| {
                        MachineError::MachineCreation(format!("connect {name} to {network}: {e}"))
                    })?;
            }
        }

        self.docker
            .start_container::<String>(&name, None)
            .await
            .map_err(|e| MachineError::MachineCreation(format!("start {name}: {e}")))?;

        // Addressing: shared mode uses the container name (Docker DNS); per-match
        // mode uses the IP on the container's own slot network, dialable from
        // the host where the coordinator runs.
        let private_ip = if per_match {
            self.container_ip(&name, &ctx.networks[slot as usize])
                .await?
        } else {
            name.clone()
        };

        tracing::info!(
            match_id = ctx.match_id,
            container = name,
            image,
            slot,
            private_ip,
            "Spawned Docker container"
        );

        Ok(MachineHandle {
            app_name: ctx.match_id.clone(),
            machine_id: name,
            private_ip,
            // Containers are addressed directly, so the consumer dials the
            // in-machine port it already knows. No host relay involved.
            grpc_port: None,
        })
    }

    async fn destroy(
        &self,
        _ctx: &DockerMatchContext,
        handle: &MachineHandle,
    ) -> Result<(), MachineError> {
        match self
            .docker
            .remove_container(
                &handle.machine_id,
                Some(RemoveContainerOptions {
                    force: true,
                    v: true,
                    ..Default::default()
                }),
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(e) if Self::is_not_found(&e) => Ok(()),
            Err(e) => Err(MachineError::Destruction(format!(
                "remove {}: {e}",
                handle.machine_id
            ))),
        }
    }

    async fn cleanup_match(&self, ctx: DockerMatchContext) -> Result<(), MachineError> {
        // Containers are already destroyed; release the slot networks (no-op
        // list in shared mode). Best-effort per network, but surface the first
        // failure so the reaper's role stays visible in logs.
        let mut first_err = None;
        for network in &ctx.networks {
            if let Err(e) = self.remove_network(network).await {
                tracing::warn!(network, error = %e, "Failed to remove match network");
                first_err.get_or_insert(e);
            }
        }
        match first_err {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }

    async fn list_orphaned(
        &self,
        prefix: &str,
        max_age: Duration,
    ) -> Result<Vec<OrphanedResource>, MachineError> {
        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions::<String> {
                all: true,
                ..Default::default()
            }))
            .await
            .map_err(|e| MachineError::Destruction(format!("list containers: {e}")))?;

        let now = SystemTime::now();
        let mut orphaned = Vec::new();

        for c in containers {
            let name = c
                .names
                .unwrap_or_default()
                .into_iter()
                .next()
                .unwrap_or_default();
            let name = name.trim_start_matches('/').to_string();
            if !name.starts_with(prefix) {
                continue;
            }

            let created_at = c
                .created
                .filter(|&s| s >= 0)
                .map(|s| UNIX_EPOCH + Duration::from_secs(s as u64))
                .unwrap_or(now);

            if now.duration_since(created_at).unwrap_or(Duration::ZERO) >= max_age {
                orphaned.push(OrphanedResource {
                    id: c.id.unwrap_or_else(|| name.clone()),
                    name,
                    created_at,
                    kind: OrphanKind::Machine,
                });
            }
        }

        // Networks are listed after containers so the reaper (which destroys in
        // order) frees them only once their containers are force-removed.
        if matches!(
            self.config.isolation,
            DockerIsolation::PerMatchNetworks { .. }
        ) {
            orphaned.extend(self.list_orphaned_networks(max_age).await?);
        }

        tracing::info!(
            count = orphaned.len(),
            prefix,
            "Docker orphan scan complete"
        );
        Ok(orphaned)
    }

    async fn destroy_orphaned(&self, resource: &OrphanedResource) -> Result<(), MachineError> {
        match resource.kind {
            OrphanKind::Machine => match self
                .docker
                .remove_container(
                    &resource.id,
                    Some(RemoveContainerOptions {
                        force: true,
                        v: true,
                        ..Default::default()
                    }),
                )
                .await
            {
                Ok(()) => Ok(()),
                Err(e) if Self::is_not_found(&e) => Ok(()),
                Err(e) => Err(MachineError::Destruction(format!(
                    "remove orphan {}: {e}",
                    resource.id
                ))),
            },
            OrphanKind::Network => self.remove_network(&resource.id).await,
        }
    }
}
