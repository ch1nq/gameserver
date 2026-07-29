//! Firecracker-containerd implementation of MachineProvider.
//!
//! # How a microVM is launched
//!
//! firecracker-containerd separates *VM* configuration from *container*
//! configuration. Networking in particular is **not** expressed via OCI spec
//! annotations — it is set on the VM through the `fccontrol` control API's
//! `CreateVM` call ([`fccontrol`](super::fccontrol)). We therefore:
//!
//! 1. Pull the image into the devmapper snapshotter (`ctr images pull`).
//! 2. Create the per-machine TAP on the match bridge ([`network::create_tap`]).
//! 3. `CreateVM` with a static network interface bound to that TAP and the
//!    slot's IP (this tells the in-VM agent to configure the guest NIC).
//! 4. `ctr run` the container with the `aws.firecracker.vm.id` annotation so it
//!    is placed inside the VM we just created. `ctr` handles snapshot creation,
//!    image-config → process args, and rootfs mounting — the parts that are
//!    awkward and error-prone to reproduce over raw gRPC.
//!
//! VM teardown is the reverse: delete the container/task, `StopVM`, delete TAP.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use containerd_client::services::v1::{
    DeleteContainerRequest, DeleteTaskRequest, KillRequest, ListContainersRequest,
};
use containerd_client::tonic::Request;
use containerd_client::with_namespace;
use prost::Message as _;

use super::fccontrol::{
    CreateVmRequest, FirecrackerMachineConfiguration, FirecrackerNetworkInterface, IpConfiguration,
    StaticNetworkConfiguration, StopVmRequest,
};

/// containerd's namespace header key for ttrpc requests (mirrors
/// `namespaces.TTRPCHeader`). Required by the control plugin, which calls
/// `NamespaceRequired` on every request.
const TTRPC_NAMESPACE_HEADER: &str = "containerd-namespace-ttrpc";

/// The ttrpc service and methods exposed by the firecracker-control plugin.
/// The vendored proto declares `service Firecracker` with no package.
const FC_SERVICE: &str = "Firecracker";
use super::{
    config::FirecrackerMachineProviderConfig,
    network::{self, MatchNetwork},
    subnet_pool::SubnetPool,
};
use crate::{
    ContainerImage, MachineError, MachineHandle, MachineProvider, OrphanedResource, SpawnConfig,
};

/// OCI annotation key read by the firecracker-containerd shim to place a
/// container inside a pre-created VM. Mirrors `firecrackeroci.VMIDAnnotationKey`.
const VMID_ANNOTATION_KEY: &str = "aws.firecracker.vm.id";

/// Firecracker-containerd implementation of [`MachineProvider`].
///
/// Each game match gets:
/// - A dedicated `/24` subnet and Linux bridge
/// - One microVM per slot (slot 0 = game host, slot 1+ = agents)
/// - Strict iptables isolation: agents may only communicate with the game host
///
/// All microVMs are run via the `aws.firecracker` containerd shim.
pub struct FirecrackerMachineProvider {
    config: FirecrackerMachineProviderConfig,
    client: containerd_client::Client,
    subnet_pool: SubnetPool,
}

/// Per-match context for the Firecracker backend.
#[derive(Debug)]
pub struct FirecrackerMatchContext {
    pub match_id: String,
    pub network: MatchNetwork,
}

impl FirecrackerMachineProvider {
    pub async fn new(config: FirecrackerMachineProviderConfig) -> Result<Self, MachineError> {
        let client = containerd_client::Client::from_path(&config.containerd_socket)
            .await
            .map_err(|e| {
                MachineError::MatchInit(format!("Failed to connect to containerd: {e}"))
            })?;

        let subnet_pool = SubnetPool::new(config.subnet_pool);

        Ok(Self {
            config,
            client,
            subnet_pool,
        })
    }

    /// Invoke a method on the firecracker-control ttrpc service.
    ///
    /// The control API (`CreateVM`/`StopVM`) is **ttrpc**, not gRPC, and is served
    /// on a *separate* socket: `<containerd-socket>.ttrpc`. We connect per call
    /// (these are infrequent), attach the containerd namespace as ttrpc metadata,
    /// and carry prost-encoded messages as the raw payload.
    async fn fc_control_call(
        &self,
        method: &str,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, MachineError> {
        let addr = format!("unix://{}.ttrpc", self.config.containerd_socket);
        let client = ttrpc::asynchronous::Client::connect(&addr)
            .await
            .map_err(|e| {
                MachineError::MachineCreation(format!("ttrpc connect to {addr} failed: {e}"))
            })?;

        let mut md = HashMap::new();
        md.insert(
            TTRPC_NAMESPACE_HEADER.to_string(),
            vec![self.config.containerd_namespace.clone()],
        );

        let req = ttrpc::Request {
            service: FC_SERVICE.to_string(),
            method: method.to_string(),
            // VM creation boots the guest; allow ample time before giving up.
            timeout_nano: 120_000_000_000,
            metadata: ttrpc::context::to_pb(md),
            payload,
            ..Default::default()
        };

        let resp = client.request(req).await.map_err(|e| {
            MachineError::MachineCreation(format!("Firecracker.{method} failed: {e}"))
        })?;
        Ok(resp.payload)
    }

    /// Pull an OCI image into containerd using `ctr`.
    ///
    /// We shell out to `ctr` because it handles the full OCI pull + unpack
    /// workflow into the devmapper snapshotter required by firecracker-containerd.
    async fn pull_image(
        &self,
        image_ref: &str,
        token: Option<&str>,
        plain_http: bool,
    ) -> Result<(), MachineError> {
        let mut cmd = tokio::process::Command::new("ctr");
        cmd.args([
            "--namespace",
            &self.config.containerd_namespace,
            "--address",
            &self.config.containerd_socket,
            "images",
            "pull",
            "--snapshotter",
            "devmapper",
        ]);

        // Only the private registry may be served over plain HTTP. Public
        // registries (ghcr.io, docker.io, …) are always HTTPS — image refs
        // never carry a scheme, so this must be decided by the caller, not by
        // sniffing the ref.
        if plain_http {
            cmd.arg("--plain-http");
        }

        // NOTE: registry auth flag depends on how the private registry accepts
        // the scoped JWT. `--token` works with registries that accept a bearer
        // token directly; validate against the deployed registry on the host.
        if let Some(t) = token {
            cmd.args(["--token", t]);
        }

        cmd.arg(image_ref);

        let output = cmd
            .output()
            .await
            .map_err(|e| MachineError::MachineCreation(format!("Failed to run ctr: {e}")))?;

        if !output.status.success() {
            return Err(MachineError::ImageCopy(format!(
                "ctr image pull failed for {image_ref}: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        tracing::info!(image = image_ref, "Image pulled into containerd");
        Ok(())
    }

    /// Create a microVM via the `fccontrol` CreateVM API, bound to a pre-created
    /// TAP and a static guest IP.
    async fn create_vm(
        &self,
        vmid: &str,
        tap_name: &str,
        guest_ip: &str,
        gateway_ip: &str,
        subnet_prefix: u8,
    ) -> Result<(), MachineError> {
        let network = FirecrackerNetworkInterface {
            static_config: Some(StaticNetworkConfiguration {
                mac_address: mac_for_ip(guest_ip),
                host_dev_name: tap_name.to_string(),
                ip_config: Some(IpConfiguration {
                    primary_addr: format!("{guest_ip}/{subnet_prefix}"),
                    gateway_addr: gateway_ip.to_string(),
                    nameservers: Vec::new(),
                }),
            }),
            ..Default::default()
        };

        // Kernel image, boot args and the VM agent rootfs come from the
        // firecracker-containerd runtime config (firecracker-runtime.json)
        // defaults; here we only pin the VM id, machine size, and networking.
        let req = CreateVmRequest {
            vmid: vmid.to_string(),
            machine_cfg: Some(FirecrackerMachineConfiguration {
                vcpu_count: self.config.vcpu_count,
                mem_size_mib: self.config.mem_size_mib,
                ..Default::default()
            }),
            network_interfaces: vec![network],
            ..Default::default()
        };

        self.fc_control_call("CreateVM", req.encode_to_vec())
            .await
            .map_err(|e| {
                MachineError::MachineCreation(format!("CreateVM failed for {vmid}: {e}"))
            })?;

        tracing::info!(vmid, tap = tap_name, guest_ip, "microVM created");
        Ok(())
    }

    /// Run the container inside a pre-created VM via `ctr run --detach`.
    ///
    /// `ctr` resolves the image config (entrypoint/cmd), prepares the devmapper
    /// snapshot, and mounts the rootfs — the `aws.firecracker.vm.id` annotation
    /// routes it into the VM created by [`Self::create_vm`].
    async fn run_container(
        &self,
        container_id: &str,
        vmid: &str,
        image_ref: &str,
        match_id: &str,
        env: &HashMap<String, String>,
    ) -> Result<(), MachineError> {
        let created_at = unix_now_secs().to_string();

        let mut cmd = tokio::process::Command::new("ctr");
        cmd.args([
            "--namespace",
            &self.config.containerd_namespace,
            "--address",
            &self.config.containerd_socket,
            "run",
            "--detach",
            // Run in the VM's "host" network namespace so the container inherits
            // the guest NIC that CreateVM statically configured (the match IP).
            // Without this the container gets an empty netns (only `lo`) and has
            // no access to the match network at all.
            "--net-host",
            "--runtime",
            &self.config.runtime,
            "--snapshotter",
            "devmapper",
            "--annotation",
            &format!("{VMID_ANNOTATION_KEY}={vmid}"),
            // Labels drive orphan reaping (see `list_orphaned`).
            "--label",
            &format!("achtung.match={match_id}"),
            "--label",
            &format!("achtung.created_at={created_at}"),
        ]);

        for (k, v) in env {
            cmd.arg("--env").arg(format!("{k}={v}"));
        }

        cmd.arg(image_ref).arg(container_id);

        let output = cmd
            .output()
            .await
            .map_err(|e| MachineError::MachineCreation(format!("Failed to run ctr run: {e}")))?;

        if !output.status.success() {
            return Err(MachineError::MachineCreation(format!(
                "ctr run failed for {container_id}: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        tracing::info!(
            container_id,
            vmid,
            image = image_ref,
            "container started in microVM"
        );
        Ok(())
    }

    /// Kill and delete a container's task and the container record (best-effort
    /// on the task, which may already be gone).
    async fn stop_and_delete_container(&self, container_id: &str) -> Result<(), MachineError> {
        let ns = self.config.containerd_namespace.as_str();

        let kill_req: Request<KillRequest> = with_namespace!(
            KillRequest {
                container_id: container_id.to_string(),
                signal: 9,
                all: true,
                ..Default::default()
            },
            ns
        );
        if let Err(e) = self.client.tasks().kill(kill_req).await {
            tracing::debug!(container_id, error = %e, "Kill task (may already be stopped)");
        }

        tokio::time::sleep(Duration::from_millis(200)).await;

        let delete_task_req: Request<DeleteTaskRequest> = with_namespace!(
            DeleteTaskRequest {
                container_id: container_id.to_string(),
            },
            ns
        );
        if let Err(e) = self.client.tasks().delete(delete_task_req).await {
            tracing::warn!(container_id, error = %e, "Failed to delete task record");
        }

        let delete_req: Request<DeleteContainerRequest> = with_namespace!(
            DeleteContainerRequest {
                id: container_id.to_string(),
            },
            ns
        );
        self.client
            .containers()
            .delete(delete_req)
            .await
            .map_err(|e| {
                MachineError::Destruction(format!("Failed to delete container {container_id}: {e}"))
            })?;

        tracing::info!(container_id, "container stopped and deleted");
        Ok(())
    }

    /// Stop a microVM via the `fccontrol` StopVM API. Best-effort.
    async fn stop_vm(&self, vmid: &str) {
        let req = StopVmRequest {
            vmid: vmid.to_string(),
            timeout_seconds: 10,
        };
        if let Err(e) = self.fc_control_call("StopVM", req.encode_to_vec()).await {
            tracing::warn!(vmid, error = %e, "StopVM failed (VM may already be gone)");
        } else {
            tracing::info!(vmid, "microVM stopped");
        }
    }
}

#[async_trait::async_trait]
impl MachineProvider for FirecrackerMachineProvider {
    type MatchContext = FirecrackerMatchContext;

    async fn init_match(&self, match_id: &str) -> Result<FirecrackerMatchContext, MachineError> {
        let subnet = self
            .subnet_pool
            .allocate()
            .map_err(|e| MachineError::MatchInit(e.to_string()))?;

        let network = network::setup(match_id, subnet)
            .await
            .map_err(|e| MachineError::MatchInit(e.to_string()))?;

        tracing::info!(
            match_id,
            subnet = %subnet,
            bridge = network.bridge_name,
            "Firecracker match initialized"
        );

        Ok(FirecrackerMatchContext {
            match_id: match_id.to_string(),
            network,
        })
    }

    async fn spawn(
        &self,
        ctx: &FirecrackerMatchContext,
        config: SpawnConfig,
    ) -> Result<MachineHandle, MachineError> {
        let slot = config.slot;
        let guest_ip = ctx.network.ip_for_slot(slot);
        let gateway_ip = ctx.network.host_ip;
        let subnet_prefix = ctx.network.subnet.prefix_len();

        let (image_ref, token, plain_http) = match &config.container_image {
            ContainerImage::Public(url) => (url.as_ref().to_string(), None, false),
            ContainerImage::Private {
                image_url,
                registry_token,
            } => {
                let registry_host = self
                    .config
                    .registry_url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://");
                let full_ref = format!("{}/{}", registry_host, image_url.as_ref());
                let plain_http = self.config.registry_url.starts_with("http://");
                (
                    full_ref,
                    Some(registry_token.as_ref().to_string()),
                    plain_http,
                )
            }
        };

        self.pull_image(&image_ref, token.as_deref(), plain_http)
            .await?;

        let tap_name = network::create_tap(&ctx.network, slot)
            .await
            .map_err(|e| MachineError::MachineCreation(e.to_string()))?;

        // Container ID doubles as the VM ID; it encodes match prefix and slot.
        let id_prefix = &ctx.match_id[..8.min(ctx.match_id.len())];
        let container_id = format!("achtung-{id_prefix}-slot-{slot}");

        self.create_vm(
            &container_id,
            &tap_name,
            &guest_ip.to_string(),
            &gateway_ip.to_string(),
            subnet_prefix,
        )
        .await?;

        if let Err(e) = self
            .run_container(
                &container_id,
                &container_id,
                &image_ref,
                &ctx.match_id,
                &config.env,
            )
            .await
        {
            // The VM was created but the container failed to start — tear the VM
            // and TAP back down so we don't leak them.
            self.stop_vm(&container_id).await;
            if let Err(e) = network::delete_tap(&tap_name).await {
                tracing::warn!(tap = tap_name, error = %e, "Failed to delete TAP after spawn failure");
            }
            return Err(e);
        }

        Ok(MachineHandle {
            app_name: ctx.match_id.clone(),
            machine_id: container_id,
            private_ip: guest_ip.to_string(),
        })
    }

    async fn destroy(
        &self,
        ctx: &FirecrackerMatchContext,
        handle: &MachineHandle,
    ) -> Result<(), MachineError> {
        // container_id == vmid
        self.stop_and_delete_container(&handle.machine_id).await?;
        self.stop_vm(&handle.machine_id).await;

        // Derive the slot from the container ID ("achtung-{prefix}-slot-{N}")
        if let Some(slot_str) = handle.machine_id.split("-slot-").last()
            && let Ok(slot) = slot_str.parse::<u8>()
        {
            let tap_name = ctx.network.tap_name(slot);
            if let Err(e) = network::delete_tap(&tap_name).await {
                tracing::warn!(
                    tap = tap_name,
                    error = %e,
                    "Failed to delete TAP (may already be gone)"
                );
            }
        }

        Ok(())
    }

    async fn cleanup_match(&self, ctx: FirecrackerMatchContext) -> Result<(), MachineError> {
        network::teardown(&ctx.network)
            .await
            .map_err(|e| MachineError::MatchCleanup(e.to_string()))?;

        self.subnet_pool.release(ctx.network.subnet);

        tracing::info!(match_id = ctx.match_id, "Firecracker match cleaned up");
        Ok(())
    }

    async fn list_orphaned(
        &self,
        prefix: &str,
        max_age: Duration,
    ) -> Result<Vec<OrphanedResource>, MachineError> {
        let ns = self.config.containerd_namespace.as_str();
        let now = SystemTime::now();

        // Match containers by id prefix (all our containers are "achtung-…-slot-N").
        // The reaper prefix for firecracker should be "achtung-" (see app config).
        let list_req: Request<ListContainersRequest> = with_namespace!(
            ListContainersRequest {
                filters: vec![format!("id~={prefix}")],
            },
            ns
        );

        let response =
            self.client.containers().list(list_req).await.map_err(|e| {
                MachineError::Destruction(format!("Failed to list containers: {e}"))
            })?;

        let mut orphaned = Vec::new();
        for container in response.into_inner().containers {
            let created_at = container
                .labels
                .get("achtung.created_at")
                .and_then(|s| s.parse::<u64>().ok())
                .map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs));

            if let Some(created_at) = created_at {
                let age = now.duration_since(created_at).unwrap_or(Duration::ZERO);
                if age >= max_age {
                    orphaned.push(OrphanedResource {
                        id: container.id.clone(),
                        name: container.id,
                        created_at,
                    });
                }
            }
        }

        tracing::info!(
            count = orphaned.len(),
            prefix,
            "Firecracker orphan scan complete"
        );
        Ok(orphaned)
    }

    async fn destroy_orphaned(&self, resource: &OrphanedResource) -> Result<(), MachineError> {
        // resource.id == container_id == vmid
        self.stop_and_delete_container(&resource.id).await?;
        self.stop_vm(&resource.id).await;
        tracing::info!(
            container_id = resource.id,
            "Destroyed orphaned Firecracker microVM"
        );
        Ok(())
    }
}

/// Seconds since the Unix epoch.
fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Derive a stable, locally-administered MAC address from an IPv4 address.
///
/// Uses the `02:fc:` locally-administered prefix followed by the four IP octets
/// so each guest gets a deterministic, collision-free MAC within a match.
fn mac_for_ip(ip: &str) -> String {
    let octets: Vec<&str> = ip.split('.').collect();
    if octets.len() == 4
        && let (Ok(a), Ok(b), Ok(c), Ok(d)) = (
            octets[0].parse::<u8>(),
            octets[1].parse::<u8>(),
            octets[2].parse::<u8>(),
            octets[3].parse::<u8>(),
        )
    {
        return format!("02:fc:{a:02x}:{b:02x}:{c:02x}:{d:02x}");
    }
    // Fallback: let firecracker auto-assign by leaving it empty.
    String::new()
}
