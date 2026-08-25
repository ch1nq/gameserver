//! Agent infrastructure management library.
//!
//! Provides abstractions for provisioning and managing agent machines
//! for game matches. Supports multiple backends (Docker/gVisor, microsandbox).

pub mod docker;
pub mod microsandbox;
pub mod reaper;

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use common::{ImageUrl, RegistryToken};
use rand::{Rng, distr::Alphanumeric};

// Re-export key types
pub use docker::{DockerIsolation, DockerMachineProvider, DockerMachineProviderConfig};
pub use microsandbox::{
    MicrosandboxMachineProvider, MicrosandboxMachineProviderConfig, ensure_runtime_installed,
};
pub use reaper::{Reaper, ReaperConfig};

#[derive(Debug, Clone)]
pub enum ContainerImage {
    Public(ImageUrl),
    Private {
        image_url: ImageUrl,
        registry_token: RegistryToken,
    },
}

/// Configuration for spawning a single machine within a match.
///
/// The `slot` determines the machine's role and network address within the match:
/// - slot 0: game host
/// - slot 1+: agents (in order)
///
/// Each backend derives the machine's IP from the slot number deterministically,
/// so no mutable state is needed to track allocations.
#[derive(Debug, Clone)]
pub struct SpawnConfig {
    /// Image to spawn
    pub container_image: ContainerImage,
    /// Environment variables to set in the container
    pub env: HashMap<String, String>,
    /// Slot index: 0 = game host, 1+ = agents.
    ///
    /// Used by backends that assign addresses deterministically per slot.
    pub slot: u8,
    /// Port the workload listens on *inside* the machine.
    ///
    /// Docker/gVisor ignore this — containers are addressed directly, so the
    /// consumer already knows the port. microsandbox needs it to publish a host
    /// port (`port(host, guest)`), since a microVM's own address is not
    /// reachable from outside its `/30`.
    ///
    /// Required rather than defaulted: a wrong value here surfaces as a
    /// connection timeout minutes later, far from its cause.
    pub grpc_port: u16,
}

impl SpawnConfig {
    /// Create a new SpawnConfig with the given container image, slot, and
    /// in-machine listen port.
    pub fn new(container_image: ContainerImage, slot: u8, grpc_port: u16) -> Self {
        Self {
            container_image,
            env: HashMap::new(),
            slot,
            grpc_port,
        }
    }

    /// Add an environment variable
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Add multiple environment variables
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env.extend(env);
        self
    }
}

/// Handle to a spawned machine, used for cleanup and addressing
#[derive(Debug, Clone)]
pub struct MachineHandle {
    /// Backend-specific identifier for grouping (e.g., match ID)
    pub app_name: String,
    /// Backend-specific machine identifier (e.g., container ID)
    pub machine_id: String,
    /// Address by which *this machine's consumer* reaches it — **not**
    /// necessarily the machine's own IP.
    ///
    /// Who the consumer is depends on the slot: the coordinator (a host
    /// process) dials the game host, and the game host (inside a guest) dials
    /// the agents. Backends are free to return whatever each consumer needs:
    ///
    /// - Docker `SharedNetwork`: the container name, resolved by Docker DNS.
    /// - Docker `PerMatchNetworks`: the container's IP on its slot network.
    /// - microsandbox: `127.0.0.1` for the game host (published port, read on
    ///   the host) and `host.microsandbox.internal` for agents (the guest-side
    ///   name for the host relay).
    pub private_ip: String,
    /// Port the consumer should dial on [`Self::private_ip`], when it differs
    /// from the port the workload listens on inside the machine.
    ///
    /// `None` means "dial the in-machine port directly" (Docker/gVisor).
    /// microsandbox sets `Some(published_host_port)` because traffic is
    /// relayed through a host port rather than sent to the guest directly.
    pub grpc_port: Option<u16>,
}

/// What kind of resource an [`OrphanedResource`] refers to, so
/// `destroy_orphaned` knows which API to delete it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanKind {
    /// A machine (container / microVM).
    Machine,
    /// Per-match network infrastructure (e.g., a Docker network).
    Network,
}

/// Information about orphaned resources to be reaped
#[derive(Debug, Clone)]
pub struct OrphanedResource {
    /// Platform-specific identifier (e.g., containerd container ID)
    pub id: String,
    /// Human-readable name for logging
    pub name: String,
    /// When the resource was created
    pub created_at: SystemTime,
    /// What kind of resource this is
    pub kind: OrphanKind,
}

/// Errors that can occur during machine operations
#[derive(Debug, thiserror::Error)]
pub enum MachineError {
    #[error("Failed to initialize match: {0}")]
    MatchInit(String),
    #[error("Failed to cleanup match: {0}")]
    MatchCleanup(String),
    #[error("Failed to create app: {0}")]
    AppCreation(String),
    #[error("Failed to assign IP: {0}")]
    IpAssignment(String),
    #[error("Failed to copy image: {0}")]
    ImageCopy(String),
    #[error("Failed to create machine: {0}")]
    MachineCreation(String),
    #[error("Failed to destroy: {0}")]
    Destruction(String),
}

/// Trait for provisioning and managing agent machines.
///
/// Implementations must be `Send + Sync + 'static` so they can be shared
/// across async tasks (coordinator + reaper) via `Arc<P>`.
///
/// # Match lifecycle
///
/// Each game match follows this sequence:
/// 1. `init_match` — allocate shared resources (network, bridge, etc.)
/// 2. `spawn` × N — start individual machines within the match
/// 3. `destroy` × N — stop individual machines
/// 4. `cleanup_match` — release shared resources
///
/// The associated `MatchContext` type carries backend-specific per-match state
/// (e.g., subnet, bridge name for firecracker; container prefix for docker).
#[async_trait::async_trait]
pub trait MachineProvider: Send + Sync + 'static {
    /// Backend-specific per-match context produced by `init_match` and
    /// consumed by `spawn`, `destroy`, and `cleanup_match`.
    type MatchContext: Send + Sync;

    /// Initialize shared resources for a match.
    ///
    /// Called once before any `spawn` calls. Sets up networking and other
    /// shared infrastructure for the match. `num_slots` is the total number of
    /// machines the match will spawn (game host + agents); backends that
    /// allocate per-slot resources up front (e.g. one network per slot) need
    /// it because resources cannot always be attached after machines start.
    async fn init_match(
        &self,
        match_id: &str,
        num_slots: u8,
    ) -> Result<Self::MatchContext, MachineError>;

    /// Spawn a single machine within an initialized match.
    ///
    /// `config.slot` determines the machine's role (0 = game host, 1+ = agents)
    /// and is used by backends that assign IPs deterministically per slot.
    async fn spawn(
        &self,
        ctx: &Self::MatchContext,
        config: SpawnConfig,
    ) -> Result<MachineHandle, MachineError>;

    /// Destroy a single machine.
    async fn destroy(
        &self,
        ctx: &Self::MatchContext,
        handle: &MachineHandle,
    ) -> Result<(), MachineError>;

    /// Clean up shared resources for a match.
    ///
    /// Called once after all machines have been destroyed. Releases networking
    /// and other resources allocated in `init_match`.
    async fn cleanup_match(&self, ctx: Self::MatchContext) -> Result<(), MachineError>;

    /// List infrastructure that matches the prefix and is older than `max_age`.
    ///
    /// Used by the reaper to find orphaned match infrastructure.
    async fn list_orphaned(
        &self,
        prefix: &str,
        max_age: Duration,
    ) -> Result<Vec<OrphanedResource>, MachineError>;

    /// Destroy orphaned infrastructure by ID.
    ///
    /// Best-effort: errors are logged but should not prevent cleanup of other
    /// orphaned resources.
    async fn destroy_orphaned(&self, resource: &OrphanedResource) -> Result<(), MachineError>;
}

pub fn generate_id() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect::<String>()
        .to_lowercase()
}
