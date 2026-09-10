//! Agent infrastructure management library.
//!
//! Provides abstractions for provisioning and managing agent machines
//! for game matches. Supports multiple backends (Docker, microsandbox microVMs).

pub mod docker;
pub mod microsandbox;
pub mod reaper;
pub mod slot;

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use common::{ImageUrl, RegistryToken};
use rand::{Rng, distr::Alphanumeric};

// Re-export key types
pub use docker::{DockerMachineProvider, DockerMachineProviderConfig};
pub use microsandbox::{
    MicrosandboxMachineProvider, MicrosandboxMachineProviderConfig, ensure_runtime_installed,
};
pub use reaper::{Reaper, ReaperConfig};
pub use slot::{AgentSlot, MatchLayout};

#[derive(Debug, Clone)]
pub enum ContainerImage {
    Public(ImageUrl),
    Private {
        image_url: ImageUrl,
        registry_token: RegistryToken,
    },
}

/// Configuration for spawning the game host within a match.
///
/// The host has no slot: its role is in the type of this struct and in the
/// [`MachineProvider::spawn_host`] method that takes it.
#[derive(Debug, Clone)]
pub struct HostSpawnConfig {
    /// Image to spawn
    pub container_image: ContainerImage,
    /// Environment variables to set in the container
    pub env: HashMap<String, String>,
    /// Port the workload listens on *inside* the machine.
    ///
    /// Backends that relay through a published host port need this to map
    /// host to guest; backends that address machines directly may ignore it.
    ///
    /// Required rather than defaulted: a wrong value here surfaces as a
    /// connection timeout minutes later, far from its cause.
    pub grpc_port: u16,
}

impl HostSpawnConfig {
    /// Create a new host spawn config with the given image and in-machine port.
    pub fn new(container_image: ContainerImage, grpc_port: u16) -> Self {
        Self {
            container_image,
            env: HashMap::new(),
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

/// Configuration for spawning a single agent within a match.
///
/// The agent's identity is an [`AgentSlot`] (0-based index), which can only be
/// obtained from a [`MatchLayout`] — so an out-of-range slot or the host slot
/// cannot be constructed here.
#[derive(Debug, Clone)]
pub struct AgentSpawnConfig {
    /// Image to spawn
    pub container_image: ContainerImage,
    /// Environment variables to set in the container
    pub env: HashMap<String, String>,
    /// Which agent this is (0-based). Determines the machine's name and
    /// network address deterministically.
    pub slot: AgentSlot,
    /// Port the workload listens on *inside* the machine. See
    /// [`HostSpawnConfig::grpc_port`].
    pub grpc_port: u16,
}

impl AgentSpawnConfig {
    /// Create a new agent spawn config with the given image, slot, and
    /// in-machine listen port.
    pub fn new(container_image: ContainerImage, slot: AgentSlot, grpc_port: u16) -> Self {
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
    /// The consumer is fixed by role: the coordinator dials the game host,
    /// and the game host dials the agents. Backends are free to return
    /// whatever each consumer needs.
    pub private_ip: String,
    /// Port the consumer should dial on [`Self::private_ip`], when it differs
    /// from the port the workload listens on inside the machine.
    ///
    /// `None` means "dial the in-machine port directly".
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
    /// Platform-specific identifier (e.g., container ID)
    pub id: String,
    /// Human-readable name for logging
    pub name: String,
    /// When the resource was created
    pub created_at: SystemTime,
    /// What kind of resource this is, so the reaper deletes it with the right API
    pub kind: OrphanKind,
}

/// Errors that can occur during machine operations
#[derive(Debug, thiserror::Error)]
pub enum MachineError {
    #[error("failed to initialize match infrastructure: {0}")]
    MatchInit(String),

    #[error("failed to create machine: {0}")]
    MachineCreation(String),

    #[error("failed to copy image: {0}")]
    ImageCopy(String),

    #[error("failed to assign IP: {0}")]
    IpAssignment(String),

    #[error("failed to destroy infrastructure: {0}")]
    Destruction(String),
}

/// Provisions and tears down the machines that make up a single game match.
///
/// # Match lifecycle
///
/// Each game match follows this sequence:
/// 1. `init_match` — allocate shared resources (network, bridge, etc.)
/// 2. `spawn_host` × 1 + `spawn_agent` × N — start machines within the match
/// 3. `destroy` × N — stop individual machines
/// 4. `cleanup_match` — release shared resources
///
/// The associated `MatchContext` type carries backend-specific per-match state
/// (e.g., the container name prefix for docker), so no shared mutable state is
/// needed to correlate the four phases.
#[async_trait::async_trait]
pub trait MachineProvider: Send + Sync + 'static {
    /// Backend-specific per-match context produced by `init_match` and
    /// consumed by `spawn_*`, `destroy`, and `cleanup_match`.
    ///
    /// `'static` so a finished match's teardown (destroy + cleanup) can move
    /// into a background task and overlap the inter-game sleep instead of
    /// blocking the next match. Both built-in contexts are owned data and
    /// already satisfy this.
    type MatchContext: Send + Sync + 'static;

    /// Initialize shared resources for a match.
    ///
    /// Called once before any `spawn_*` calls. Sets up networking and other
    /// shared infrastructure for the match. `layout` is the validated match
    /// shape (host + >=1 agent); backends that allocate per-agent resources up
    /// front need it because resources cannot always be attached after machines
    /// start. Port-range overflow is rejected here, not at spawn time.
    async fn init_match(
        &self,
        match_id: &str,
        layout: MatchLayout,
    ) -> Result<Self::MatchContext, MachineError>;

    /// Spawn the game host within an initialized match. Called exactly once.
    async fn spawn_host(
        &self,
        ctx: &Self::MatchContext,
        config: HostSpawnConfig,
    ) -> Result<MachineHandle, MachineError>;

    /// Spawn a single agent within an initialized match.
    ///
    /// `config.slot` comes from the same [`MatchLayout`] passed to
    /// `init_match`, so it is always in range.
    async fn spawn_agent(
        &self,
        ctx: &Self::MatchContext,
        config: AgentSpawnConfig,
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
