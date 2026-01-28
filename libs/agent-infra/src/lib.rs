//! Agent infrastructure management library.
//!
//! Provides abstractions for provisioning and managing agent machines
//! for game matches.

mod fly_api;
pub mod registry_client;

use std::collections::HashMap;

use fly_api::{FlyApi, FlyHost, FlyIpType, FlyMachineConfig, FlyRestartConfig, FlyRestartPolicy};
use rand::{Rng, distr::Alphanumeric};
use registry_client::{BasicRegistryCredentials, RegistryClient};

/// Configuration for spawning a machine
#[derive(Debug, Clone)]
pub struct SpawnConfig {
    /// The container image URL (e.g., "user-123/my-agent:v1")
    pub image_url: String,
    /// Registry token for pulling the image
    pub registry_token: String,
    /// Environment variables to set in the container
    pub env: HashMap<String, String>,
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
#[trait_variant::make(MachineProvider: Send)]
pub trait LocalMachineProvider {
    /// Spawn a new machine for an agent.
    ///
    /// This creates all necessary infrastructure (app, network, IP) and
    /// starts the machine with the given container image.
    async fn spawn(&self, config: SpawnConfig) -> Result<MachineHandle, MachineError>;

    /// Destroy a machine and its associated infrastructure.
    async fn destroy(&self, handle: &MachineHandle) -> Result<(), MachineError>;
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

impl MachineProvider for FlyMachineProvider {
    async fn spawn(&self, config: SpawnConfig) -> Result<MachineHandle, MachineError> {
        // Generate unique identifiers
        let id = generate_id();
        let app_name = format!("achtung-match-{}-app", id);
        let network = format!("achtung-match-{}-net", id);

        // 1. Create Fly app with network
        self.fly_api
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

        // 3. Copy image from source registry to Fly registry
        let registry_host = self
            .config
            .registry_url
            .split_once("://")
            .map(|(_, host)| host)
            .unwrap_or(&self.config.registry_url);
        let source_image = format!("{}/{}", registry_host, config.image_url);
        let destination_image = format!("registry.fly.io/{}", app_name);

        self.registry_client
            .copy_image(
                &source_image,
                &destination_image,
                &config.registry_token,
                &BasicRegistryCredentials {
                    username: "x".into(),
                    password: self.config.fly_token.clone(),
                },
            )
            .await
            .map_err(|e| MachineError::ImageCopy(e))?;

        // 4. Create and start machine
        let machine_config = FlyMachineConfig {
            image: destination_image,
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
}

fn generate_id() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect::<String>()
        .to_lowercase()
}
