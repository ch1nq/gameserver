//! Agent infrastructure management library.
//!
//! Provides abstractions for provisioning and managing agent machines
//! for game matches. Supports multiple backends (Fly.io, Firecracker).

pub mod firecracker;
mod fly_api;
pub mod reaper;
pub mod registry_client;

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use common::{ImageUrl, RegistryToken};
use fly_api::{FlyApi, FlyHost, FlyIpType, FlyMachineConfig, FlyRestartConfig, FlyRestartPolicy};
use rand::{Rng, distr::Alphanumeric};
use registry_client::{BasicRegistryCredentials, RegistryClient};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

// Re-export key types
pub use firecracker::{FirecrackerMachineProvider, FirecrackerMachineProviderConfig};
pub use reaper::{Reaper, ReaperConfig};

/// Parse an ISO 8601 timestamp string to SystemTime
fn parse_iso8601_to_system_time(s: &str) -> Option<SystemTime> {
    let dt = OffsetDateTime::parse(s, &Rfc3339).ok()?;
    let unix_timestamp = dt.unix_timestamp();
    if unix_timestamp >= 0 {
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(unix_timestamp as u64))
    } else {
        None
    }
}

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
    /// Used by the firecracker backend to assign deterministic IPs within the
    /// match subnet. Fly.io ignores this — each machine gets its own app and IP.
    pub slot: u8,
}

impl SpawnConfig {
    /// Create a new SpawnConfig with the given container image and slot
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
    /// Backend-specific identifier for grouping (e.g., Fly app name, match ID)
    pub app_name: String,
    /// Backend-specific machine identifier (e.g., Fly machine ID, container ID)
    pub machine_id: String,
    /// IP address for gRPC communication (without brackets)
    pub private_ip: String,
}

/// Information about orphaned resources to be reaped
#[derive(Debug, Clone)]
pub struct OrphanedResource {
    /// Platform-specific identifier (e.g., Fly app name, containerd container ID)
    pub id: String,
    /// Human-readable name for logging
    pub name: String,
    /// When the resource was created
    pub created_at: SystemTime,
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
/// (e.g., subnet, bridge name for firecracker; app name for Fly.io).
#[async_trait::async_trait]
pub trait MachineProvider: Send + Sync + 'static {
    /// Backend-specific per-match context produced by `init_match` and
    /// consumed by `spawn`, `destroy`, and `cleanup_match`.
    type MatchContext: Send + Sync;

    /// Initialize shared resources for a match.
    ///
    /// Called once before any `spawn` calls. Sets up networking and other
    /// shared infrastructure for the match.
    async fn init_match(&self, match_id: &str) -> Result<Self::MatchContext, MachineError>;

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

/// Configuration for the Fly.io machine provider
#[derive(Debug, Clone)]
pub struct FlyMachineProviderConfig {
    /// Fly.io API token
    pub fly_token: String,
    /// Fly.io organization slug for creating apps
    pub fly_org: String,
    /// Whether to use internal or public Fly API
    pub fly_host: FlyMachineProviderHost,
    /// URL of the source registry (e.g., "https://achtung-registry.fly.dev")
    pub registry_url: String,
}

/// Which Fly API endpoint to use
#[derive(Debug, Clone)]
pub enum FlyMachineProviderHost {
    /// Use internal Fly API (from within Fly network)
    Internal,
    /// Use public Fly API
    Public,
}

/// Per-match context for the Fly.io backend.
///
/// One Fly app is created per match; all machines (game host + agents) run
/// within it and share its private network.
#[derive(Debug, Clone)]
pub struct FlyMatchContext {
    /// The Fly app name for this match (e.g., "achtung-match-abc123")
    pub app_name: String,
    /// The Fly network name for this match
    pub network: String,
}

/// Fly.io implementation of MachineProvider
#[derive(Debug)]
pub struct FlyMachineProvider {
    fly_api: FlyApi,
    registry_client: RegistryClient,
    config: FlyMachineProviderConfig,
}

impl FlyMachineProvider {
    pub fn new(config: FlyMachineProviderConfig) -> Self {
        let http_client = reqwest::Client::new();
        let fly_host = match config.fly_host {
            FlyMachineProviderHost::Internal => FlyHost::Internal,
            FlyMachineProviderHost::Public => FlyHost::Public,
        };
        let fly_api = FlyApi::new(config.fly_token.clone(), http_client.clone(), fly_host);
        let registry_client = RegistryClient::new(config.registry_url.clone(), http_client);

        Self {
            fly_api,
            registry_client,
            config,
        }
    }
}

#[async_trait::async_trait]
impl MachineProvider for FlyMachineProvider {
    type MatchContext = FlyMatchContext;

    async fn init_match(&self, match_id: &str) -> Result<FlyMatchContext, MachineError> {
        let app_name = format!("achtung-match-{}-app", match_id);
        let network = format!("achtung-match-{}-net", match_id);

        // Create one Fly app for the whole match; all machines share its network
        self.fly_api
            .create_app(
                app_name.clone(),
                self.config.fly_org.clone(),
                network.clone(),
            )
            .await
            .map_err(MachineError::AppCreation)?;

        // Assign a private IPv6 block to the app
        self.fly_api
            .assign_ip(
                app_name.clone(),
                network.clone(),
                self.config.fly_org.clone(),
                "agent".into(),
                FlyIpType::PrivateV6,
            )
            .await
            .map_err(MachineError::IpAssignment)?;

        tracing::info!(match_id, app_name, "Fly match initialized");

        Ok(FlyMatchContext { app_name, network })
    }

    async fn spawn(
        &self,
        ctx: &FlyMatchContext,
        config: SpawnConfig,
    ) -> Result<MachineHandle, MachineError> {
        // Copy image to fly registry if it's in a private repo
        let final_image: String = match config.container_image {
            ContainerImage::Public(image_url) => {
                tracing::info!(image = image_url.as_ref(), "Using public image directly");
                image_url.as_ref().to_string()
            }
            ContainerImage::Private {
                image_url,
                registry_token,
            } => {
                let registry_host = self
                    .config
                    .registry_url
                    .split_once("://")
                    .map(|(_, host)| host)
                    .unwrap_or(&self.config.registry_url);
                let source_image =
                    ImageUrl::from(format!("{}/{}", registry_host, image_url.as_ref()));
                // Use the app name + slot to produce a unique image tag per machine
                let destination_image = ImageUrl::from(format!(
                    "registry.fly.io/{}/slot-{}",
                    ctx.app_name, config.slot
                ));

                tracing::info!(
                    from = source_image.as_ref(),
                    to = destination_image.as_ref(),
                    "Copying image to Fly registry"
                );

                self.registry_client
                    .copy_image(
                        &source_image,
                        &destination_image,
                        &registry_token,
                        &BasicRegistryCredentials {
                            username: "x".into(),
                            password: self.config.fly_token.clone(),
                        },
                    )
                    .await
                    .map_err(MachineError::ImageCopy)?;

                destination_image.as_ref().to_string()
            }
        };

        let machine_config = FlyMachineConfig {
            image: final_image,
            env: config.env,
            auto_destroy: true,
            restart: FlyRestartConfig {
                max_retries: 1,
                policy: FlyRestartPolicy::OnFailure,
            },
        };

        let machine = self
            .fly_api
            .create_machine(ctx.app_name.clone(), machine_config)
            .await
            .map_err(MachineError::MachineCreation)?;

        tracing::info!(
            app = ctx.app_name,
            machine_id = machine.id,
            ip = machine.private_ip,
            slot = config.slot,
            "Spawned Fly machine"
        );

        Ok(MachineHandle {
            app_name: ctx.app_name.clone(),
            machine_id: machine.id,
            private_ip: machine.private_ip,
        })
    }

    async fn destroy(
        &self,
        _ctx: &FlyMatchContext,
        handle: &MachineHandle,
    ) -> Result<(), MachineError> {
        // Individual machines are auto-destroyed when the app is deleted in
        // cleanup_match; nothing to do per-machine on Fly.
        tracing::debug!(
            machine_id = handle.machine_id,
            "Fly per-machine destroy is a no-op (app cleanup handles it)"
        );
        Ok(())
    }

    async fn cleanup_match(&self, ctx: FlyMatchContext) -> Result<(), MachineError> {
        // Destroying the app cascades to all machines within it
        self.fly_api
            .destroy_app(ctx.app_name.clone())
            .await
            .map_err(MachineError::Destruction)?;

        tracing::info!(app = ctx.app_name, "Fly match cleaned up");
        Ok(())
    }

    async fn list_orphaned(
        &self,
        prefix: &str,
        max_age: Duration,
    ) -> Result<Vec<OrphanedResource>, MachineError> {
        let apps_response = self
            .fly_api
            .list_apps(self.config.fly_org.clone())
            .await
            .map_err(|e| MachineError::AppCreation(format!("Failed to list apps: {}", e)))?;

        let mut orphaned = Vec::new();
        let now = SystemTime::now();

        for app in apps_response.apps {
            if !app.name.starts_with(prefix) {
                continue;
            }

            let machines = match self.fly_api.list_machines(app.name.clone()).await {
                Ok(machines) => machines,
                Err(e) => {
                    tracing::warn!(app = %app.name, error = %e, "Failed to list machines, skipping");
                    continue;
                }
            };

            if let Some(oldest_created_at) = machines
                .iter()
                .filter_map(|m| parse_iso8601_to_system_time(&m.created_at))
                .min()
            {
                let age = now
                    .duration_since(oldest_created_at)
                    .unwrap_or(Duration::ZERO);
                if age >= max_age {
                    tracing::info!(
                        app = %app.name,
                        machine_count = machines.len(),
                        age_secs = age.as_secs(),
                        "Found orphaned Fly app"
                    );
                    orphaned.push(OrphanedResource {
                        id: app.name.clone(),
                        name: app.name,
                        created_at: oldest_created_at,
                    });
                }
            }
        }

        tracing::info!(count = orphaned.len(), prefix, "Fly orphan scan complete");
        Ok(orphaned)
    }

    async fn destroy_orphaned(&self, resource: &OrphanedResource) -> Result<(), MachineError> {
        self.fly_api
            .destroy_app(resource.id.clone())
            .await
            .map_err(MachineError::Destruction)?;

        tracing::info!(app = resource.name, "Destroyed orphaned Fly app");
        Ok(())
    }
}

pub fn generate_id() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect::<String>()
        .to_lowercase()
}
