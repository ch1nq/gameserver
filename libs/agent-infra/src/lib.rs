//! Agent infrastructure management library.
//!
//! Provides abstractions for provisioning and managing agent machines
//! for game matches. Supports multiple backends (Docker, gVisor).

pub mod docker;
pub mod reaper;

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use common::{ImageUrl, RegistryToken};
use rand::{Rng, distr::Alphanumeric};

// Re-export key types
pub use docker::{DockerIsolation, DockerMachineProvider, DockerMachineProviderConfig};
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
/// Each backend derives the machine's address from the slot number
/// deterministically, so no mutable state is needed to track allocations.
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
}

impl SpawnConfig {
    /// Create a new SpawnConfig with the given container image and slot.
    pub fn new(container_image: ContainerImage, slot: u8) -> Self {
        Self {
            container_image,
            env: HashMap::new(),
            slot,
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
    /// Who the consumer is depends on the slot: the coordinator dials the game
    /// host, and the game host dials the agents. Backends are free to return
    /// whatever each consumer needs:
    ///
    /// - Docker [`DockerIsolation::SharedNetwork`]: the container name, resolved
    ///   by Docker's embedded DNS.
    /// - Docker [`DockerIsolation::PerMatchNetworks`]: the container's IP on its
    ///   slot network, since the coordinator runs outside Docker DNS.
    pub private_ip: String,
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
/// 2. `spawn` × N — start individual machines within the match
/// 3. `destroy` × N — stop individual machines
/// 4. `cleanup_match` — release shared resources
///
/// The associated `MatchContext` type carries backend-specific per-match state
/// (e.g., the slot networks for docker), so no shared mutable state is needed to
/// correlate the four phases.
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
    /// and is used by backends that assign addresses deterministically per slot.
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
