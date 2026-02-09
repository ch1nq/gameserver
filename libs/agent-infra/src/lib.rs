//! Agent infrastructure management library.
//!
//! Provides abstractions for provisioning and managing agent machines
//! for game matches.

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

// Re-export reaper types for convenience
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

/// Configuration for spawning a machine
#[derive(Debug, Clone)]
pub struct SpawnConfig {
    /// Image to spawn
    pub container_image: ContainerImage,
    /// Environment variables to set in the container
    pub env: HashMap<String, String>,
}

impl SpawnConfig {
    /// Create a new SpawnConfig with the given container image
    pub fn new(container_image: ContainerImage) -> Self {
        Self {
            container_image,
            env: HashMap::new(),
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

/// Handle to a spawned machine, used for cleanup
#[derive(Debug, Clone)]
pub struct MachineHandle {
    /// The Fly app name
    pub app_name: String,
    /// The Fly machine ID
    pub machine_id: String,
    /// Private IP address for gRPC communication
    pub private_ip: String,
}

/// Information about orphaned resources to be reaped
#[derive(Debug, Clone)]
pub struct OrphanedResource {
    /// Platform-specific identifier (e.g., Fly app name)
    pub id: String,
    /// Human-readable name for logging
    pub name: String,
    /// When the resource was created
    pub created_at: SystemTime,
}

/// Errors that can occur during machine operations
#[derive(Debug, Clone)]
pub enum MachineError {
    /// Failed to create the Fly app
    AppCreation(String),
    /// Failed to assign IP to the app
    IpAssignment(String),
    /// Failed to copy image to Fly registry
    ImageCopy(String),
    /// Failed to create the machine
    MachineCreation(String),
    /// Failed to destroy the app/machine
    Destruction(String),
}

impl std::fmt::Display for MachineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MachineError::AppCreation(e) => write!(f, "Failed to create app: {}", e),
            MachineError::IpAssignment(e) => write!(f, "Failed to assign IP: {}", e),
            MachineError::ImageCopy(e) => write!(f, "Failed to copy image: {}", e),
            MachineError::MachineCreation(e) => write!(f, "Failed to create machine: {}", e),
            MachineError::Destruction(e) => write!(f, "Failed to destroy: {}", e),
        }
    }
}

impl std::error::Error for MachineError {}

/// Trait for provisioning and managing agent machines
#[async_trait::async_trait]
pub trait MachineProvider: Send + Sync {
    /// Spawn a new machine for an agent.
    ///
    /// This creates all necessary infrastructure (app, network, IP) and
    /// starts the machine with the given container image.
    async fn spawn(&self, config: SpawnConfig) -> Result<MachineHandle, MachineError>;

    /// Destroy a machine and its associated infrastructure.
    async fn destroy(&self, handle: &MachineHandle) -> Result<(), MachineError>;

    /// List infrastructure (apps/machines) that match the given prefix pattern
    /// and are older than the given age threshold.
    ///
    /// This is used by the reaper to find orphaned match infrastructure that
    /// failed to clean up properly.
    async fn list_orphaned(
        &self,
        prefix: &str,
        max_age: Duration,
    ) -> Result<Vec<OrphanedResource>, MachineError>;

    /// Destroy orphaned infrastructure by ID.
    ///
    /// This is a best-effort operation - errors are logged but should not
    /// prevent other orphaned infrastructure from being cleaned up.
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
    async fn spawn(&self, config: SpawnConfig) -> Result<MachineHandle, MachineError> {
        // Generate unique identifiers
        let id = generate_id();
        let app_name = format!("achtung-match-{}-app", id);
        let network = format!("achtung-match-{}-net", id);

        // 1. Create Fly app with network
        let _app_response = self
            .fly_api
            .create_app(
                app_name.clone(),
                self.config.fly_org.clone(),
                network.clone(),
            )
            .await
            .map_err(|e| MachineError::AppCreation(e))?;

        // 2. Assign private IPv6 to the app
        self.fly_api
            .assign_ip(
                app_name.clone(),
                network.clone(),
                self.config.fly_org.clone(),
                "agent".into(),
                FlyIpType::PrivateV6,
            )
            .await
            .map_err(|e| MachineError::IpAssignment(e))?;

        // 3. Copy image to fly registry if it's in a private repo
        let final_image: String = match config.container_image {
            ContainerImage::Public(image_url) => {
                tracing::info!(
                    "Using image directly (skip_registry_copy=true): {}",
                    image_url.as_ref()
                );
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
                let destination_image = ImageUrl::from(format!("registry.fly.io/{}", app_name));

                tracing::info!(
                    "Copying image from {} to {}",
                    source_image.as_ref(),
                    destination_image.as_ref()
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
                    .map_err(|e| MachineError::ImageCopy(e))?;

                destination_image.as_ref().to_string()
            }
        };

        // 4. Create and start machine
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
            .create_machine(app_name.clone(), machine_config)
            .await
            .map_err(|e| MachineError::MachineCreation(e))?;

        tracing::info!(
            "Spawned machine: app={}, machine_id={}, ip={}",
            app_name,
            machine.id,
            machine.private_ip
        );

        Ok(MachineHandle {
            app_name,
            machine_id: machine.id,
            private_ip: machine.private_ip,
        })
    }

    async fn destroy(&self, handle: &MachineHandle) -> Result<(), MachineError> {
        // Destroying the app also destroys all machines within it
        self.fly_api
            .destroy_app(handle.app_name.clone())
            .await
            .map_err(|e| MachineError::Destruction(e))?;

        tracing::info!("Destroyed machine: app={}", handle.app_name);
        Ok(())
    }

    async fn list_orphaned(
        &self,
        prefix: &str,
        max_age: Duration,
    ) -> Result<Vec<OrphanedResource>, MachineError> {
        // List all apps in the organization
        let apps_response = self
            .fly_api
            .list_apps(self.config.fly_org.clone())
            .await
            .map_err(|e| MachineError::AppCreation(format!("Failed to list apps: {}", e)))?;

        let mut orphaned = Vec::new();

        for app in apps_response.apps {
            // Filter to apps matching the prefix
            if !app.name.starts_with(prefix) {
                continue;
            }

            // List machines for this app
            let machines = match self.fly_api.list_machines(app.name.clone()).await {
                Ok(machines) => machines,
                Err(e) => {
                    tracing::warn!(
                        app = %app.name,
                        error = %e,
                        "Failed to list machines, skipping"
                    );
                    continue;
                }
            };

            if let Some(oldest_created_at) = machines
                .iter()
                .filter_map(|m| parse_iso8601_to_system_time(&m.created_at))
                .min()
            {
                tracing::info!(
                    app = %app.name,
                    id = %app.id,
                    machine_count = machines.len(),
                    "Found orphaned app (has machines older than max_age)"
                );
                orphaned.push(OrphanedResource {
                    id: app.name.clone(),
                    name: app.name,
                    created_at: oldest_created_at,
                });
            }
        }

        tracing::info!(
            "Found {} orphaned apps with prefix '{}' older than {:?}",
            orphaned.len(),
            prefix,
            max_age
        );
        Ok(orphaned)
    }

    async fn destroy_orphaned(&self, resource: &OrphanedResource) -> Result<(), MachineError> {
        // The OrphanedResource.id is the app_name for Fly
        self.fly_api
            .destroy_app(resource.id.clone())
            .await
            .map_err(|e| MachineError::Destruction(e))?;

        tracing::info!("Destroyed orphaned app: {}", resource.name);
        Ok(())
    }
}

fn generate_id() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect::<String>()
        .to_lowercase()
}
