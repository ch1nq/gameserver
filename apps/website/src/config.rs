//! One typed configuration for the website, parsed once at startup with
//! fail-fast validation.
//!
//! Everything the server needs is read here via [`Config::load`] and validated
//! up front: a missing required setting, an unknown `MACHINE_PROVIDER`, or an
//! unparseable image URL fails *before* any `TcpListener`/DB work, with a single
//! human-readable error instead of a panic deep inside `serve`.
//!
//! Settings are layered by [`figment`]: a `config.toml` file (see
//! `config.example.toml`) provides the base, and environment variables override
//! it — so a deployment can commit non-secret defaults in the file and inject
//! secrets via the environment. Both use the same flat keys (`github_client_id`
//! in TOML, `GITHUB_CLIENT_ID` in the environment).
//!
//! The raw layer ([`RawConfig`]) is a flat DTO that mirrors those keys
//! one-for-one; the typed layer ([`Config`]) groups them into domain structs
//! and owns all validation.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use agent_infra::{DockerMachineProviderConfig, MicrosandboxMachineProviderConfig, ReaperConfig};
use coordinator::{CoordinatorConfig, ImageUrl};
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;

/// Config file path, overridable with `CONFIG_FILE`. A missing file is not an
/// error — the environment alone can supply every setting.
const DEFAULT_CONFIG_FILE: &str = "config.toml";

/// Ports the coordinator dials *inside* each machine. Not env-configurable:
/// the game host and agent images bake these in, so exposing them as knobs
/// would only invite drift between image and coordinator.
const GAME_HOST_GRPC_PORT: u16 = 50051;
const AGENT_GRPC_PORT: u16 = 50052;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid configuration: {0}")]
    Extract(#[from] figment::Error),

    #[error("GAME_HOST_IMAGE is not a valid image URL: {0}")]
    InvalidGameHostImage(String),

    #[error(
        "DOCKER_NETWORK is required when the coordinator is enabled with MACHINE_PROVIDER=docker"
    )]
    MissingDockerNetwork,

    #[error("HOST/PORT do not form a valid socket address ({host}:{port}): {source}")]
    InvalidSocketAddr {
        host: String,
        port: u16,
        source: std::net::AddrParseError,
    },
}

/// Machine backend selector. Deriving `Deserialize` lets figment reject an
/// unknown `MACHINE_PROVIDER` for us, with its own "unknown variant" error —
/// no hand-written match or error variant needed.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MachineProviderKind {
    Docker,
    Microsandbox,
}

/// Flat DTO mirroring the raw environment. Field names are the lowercased env
/// var names, so figment maps `GITHUB_CLIENT_ID` -> `github_client_id`, etc.
/// Fields without a `#[serde(default)]` are required and fail fast if unset.
#[derive(Debug, Deserialize)]
struct RawConfig {
    // ── Required ──────────────────────────────────────────────────────────
    github_client_id: String,
    github_client_secret: String,
    database_url: String,
    registry_private_key: String,

    // ── Registry addressing ───────────────────────────────────────────────
    #[serde(default = "default_registry_service")]
    registry_service: String,
    #[serde(default = "default_registry_url")]
    registry_url: String,
    #[serde(default = "default_registry_public_host")]
    registry_public_host: String,

    // ── Server bind ───────────────────────────────────────────────────────
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,

    // ── Coordinator (all read only when the coordinator is enabled) ───────
    #[serde(default)]
    enable_coordinator: Option<String>,
    #[serde(default = "default_machine_provider")]
    machine_provider: MachineProviderKind,
    #[serde(default = "default_game_host_image")]
    game_host_image: String,
    #[serde(default = "default_agents_per_game")]
    agents_per_game: usize,
    #[serde(default = "default_tick_rate_ms")]
    game_tick_rate_ms: u64,
    #[serde(default = "default_game_interval_secs")]
    game_interval_secs: u64,
    #[serde(default = "default_connect_timeout_secs")]
    game_host_connect_timeout_secs: u64,
    #[serde(default = "default_arena_dim")]
    arena_width: u32,
    #[serde(default = "default_arena_dim")]
    arena_height: u32,
    #[serde(default = "default_name_prefix")]
    agent_name_prefix: String,
    #[serde(default = "default_registry_pull_host")]
    docker_registry_pull_host: String,

    // ── Docker backend ────────────────────────────────────────────────────
    #[serde(default)]
    docker_network: Option<String>,

    // ── microsandbox backend ──────────────────────────────────────────────
    #[serde(default = "default_machine_cpus")]
    machine_cpus: u8,
    #[serde(default = "default_machine_mem_mib")]
    machine_mem_mib: u32,
    #[serde(default = "default_msb_host_port_base")]
    msb_host_port_base: u16,
    #[serde(default = "default_msb_host_bind")]
    msb_host_bind: IpAddr,
    #[serde(default)]
    msb_registry_insecure: Option<String>,
    #[serde(default)]
    msb_max_duration_secs: Option<u64>,

    // ── Reaper ────────────────────────────────────────────────────────────
    #[serde(default = "default_reaper_interval_secs")]
    reaper_interval_secs: u64,
    #[serde(default = "default_reaper_max_age_secs")]
    reaper_max_age_secs: u64,
    #[serde(default)]
    reaper_prefix: Option<String>,
}

fn default_registry_service() -> String {
    "registry:5001".to_string()
}
fn default_registry_url() -> String {
    "http://localhost:5001".to_string()
}
fn default_registry_public_host() -> String {
    "localhost:5001".to_string()
}
fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    3000
}
fn default_machine_provider() -> MachineProviderKind {
    MachineProviderKind::Microsandbox
}
fn default_game_host_image() -> String {
    "ghcr.io/ch1nq/achtung-game-host:latest".to_string()
}
fn default_agents_per_game() -> usize {
    4
}
fn default_tick_rate_ms() -> u64 {
    50
}
fn default_game_interval_secs() -> u64 {
    10
}
fn default_connect_timeout_secs() -> u64 {
    60
}
fn default_arena_dim() -> u32 {
    1000
}
fn default_name_prefix() -> String {
    "achtung-".to_string()
}
fn default_registry_pull_host() -> String {
    "localhost:5001".to_string()
}
fn default_machine_cpus() -> u8 {
    1
}
fn default_machine_mem_mib() -> u32 {
    512
}
fn default_msb_host_port_base() -> u16 {
    51000
}
fn default_msb_host_bind() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}
fn default_reaper_interval_secs() -> u64 {
    300
}
fn default_reaper_max_age_secs() -> u64 {
    3600
}

/// Parse a loosely-typed env flag. Presence alone is not enough — an explicit
/// `ENABLE_COORDINATOR=false` disables, matching the `.env.example` convention.
fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Fully validated website configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub github: GithubConfig,
    pub database_url: String,
    pub registry: RegistryConfig,
    /// `None` when `ENABLE_COORDINATOR` is unset/false: the server runs without
    /// spawning matches (the spectator endpoint still exists, just idle).
    pub coordinator: Option<CoordinatorSettings>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct GithubConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// RSA private key (PEM) used to mint registry auth tokens.
    pub private_key_pem: String,
    /// JWT audience; must equal the registry's `REGISTRY_AUTH_TOKEN_SERVICE`.
    pub service: String,
    /// How this server reaches the registry API.
    pub url: String,
    /// Host rendered into user-facing `docker login/tag/push` hints; must be
    /// reachable from the user's machine, never the in-network name.
    pub public_host: String,
}

/// Everything needed to spawn the coordinator + reaper. Owns the fully-built
/// provider config so `app.rs` never touches the environment.
#[derive(Debug, Clone)]
pub struct CoordinatorSettings {
    pub game_host_image: ImageUrl,
    pub agents_per_game: usize,
    pub tick_rate_ms: u64,
    pub arena_width: u32,
    pub arena_height: u32,
    pub game_interval: Duration,
    pub connect_timeout: Duration,
    /// Prefix applied to spawned machine names; shared with the reaper's match
    /// prefix so spawned and reaped names cannot drift apart.
    pub name_prefix: String,
    pub provider: ProviderConfig,
    pub reaper: ReaperConfig,
}

/// Machine backend selected by `MACHINE_PROVIDER`, carrying its fully-parsed
/// config. Building this enum is where an unknown provider fails.
#[derive(Debug, Clone)]
pub enum ProviderConfig {
    Docker(DockerMachineProviderConfig),
    Microsandbox(MicrosandboxMachineProviderConfig),
}

impl Config {
    /// Parse and validate configuration, layering `config.toml` (base) under
    /// environment-variable overrides.
    pub fn load() -> Result<Self, ConfigError> {
        let path = std::env::var("CONFIG_FILE").unwrap_or_else(|_| DEFAULT_CONFIG_FILE.to_string());
        let raw: RawConfig = Figment::new()
            .merge(Toml::file(path))
            .merge(Env::raw())
            .extract()?;
        Self::from_raw(raw)
    }

    fn from_raw(raw: RawConfig) -> Result<Self, ConfigError> {
        let coordinator = if raw
            .enable_coordinator
            .as_deref()
            .map(truthy)
            .unwrap_or(false)
        {
            Some(Self::build_coordinator(&raw)?)
        } else {
            None
        };

        Ok(Self {
            server: ServerConfig {
                host: raw.host,
                port: raw.port,
            },
            github: GithubConfig {
                client_id: raw.github_client_id,
                client_secret: raw.github_client_secret,
            },
            database_url: raw.database_url,
            registry: RegistryConfig {
                private_key_pem: raw.registry_private_key,
                service: raw.registry_service,
                url: raw.registry_url,
                public_host: raw.registry_public_host,
            },
            coordinator,
        })
    }

    fn build_coordinator(raw: &RawConfig) -> Result<CoordinatorSettings, ConfigError> {
        let game_host_image = ImageUrl::new(raw.game_host_image.clone())
            .map_err(|e| ConfigError::InvalidGameHostImage(e.to_string()))?;

        let provider = match raw.machine_provider {
            MachineProviderKind::Docker => ProviderConfig::Docker(DockerMachineProviderConfig {
                network: raw
                    .docker_network
                    .clone()
                    .ok_or(ConfigError::MissingDockerNetwork)?,
                registry_pull_host: raw.docker_registry_pull_host.clone(),
                name_prefix: raw.agent_name_prefix.clone(),
            }),
            MachineProviderKind::Microsandbox => {
                ProviderConfig::Microsandbox(MicrosandboxMachineProviderConfig {
                    cpus: raw.machine_cpus,
                    memory_mib: raw.machine_mem_mib,
                    host_port_base: raw.msb_host_port_base,
                    host_bind: raw.msb_host_bind,
                    registry_pull_host: raw.docker_registry_pull_host.clone(),
                    registry_insecure: raw
                        .msb_registry_insecure
                        .as_deref()
                        .map(truthy)
                        .unwrap_or(false),
                    max_duration_secs: raw.msb_max_duration_secs,
                })
            }
        };

        let reaper = ReaperConfig {
            interval: Duration::from_secs(raw.reaper_interval_secs),
            max_age: Duration::from_secs(raw.reaper_max_age_secs),
            prefix: raw
                .reaper_prefix
                .clone()
                .unwrap_or_else(|| raw.agent_name_prefix.clone()),
        };

        Ok(CoordinatorSettings {
            game_host_image,
            agents_per_game: raw.agents_per_game,
            tick_rate_ms: raw.game_tick_rate_ms,
            arena_width: raw.arena_width,
            arena_height: raw.arena_height,
            game_interval: Duration::from_secs(raw.game_interval_secs),
            connect_timeout: Duration::from_secs(raw.game_host_connect_timeout_secs),
            name_prefix: raw.agent_name_prefix.clone(),
            provider,
            reaper,
        })
    }
}

impl ServerConfig {
    /// Resolve the `HOST:PORT` bind address, failing fast on a malformed host.
    pub fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
        format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|source| ConfigError::InvalidSocketAddr {
                host: self.host.clone(),
                port: self.port,
                source,
            })
    }
}

impl CoordinatorSettings {
    /// Build the coordinator's own config. `poll_interval` and the in-machine
    /// gRPC ports are not env-configurable, so they are filled in here.
    pub fn coordinator_config(&self) -> CoordinatorConfig {
        CoordinatorConfig {
            game_host_image: self.game_host_image.clone(),
            agents_per_game: self.agents_per_game,
            tick_rate_ms: self.tick_rate_ms,
            arena_width: self.arena_width,
            arena_height: self.arena_height,
            game_interval: self.game_interval,
            poll_interval: Duration::from_secs(1),
            game_host_grpc_port: GAME_HOST_GRPC_PORT,
            agent_grpc_port: AGENT_GRPC_PORT,
            game_host_connect_timeout: self.connect_timeout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A raw config with only the required fields set and everything else at its
    /// documented default — the equivalent of a minimal `.env`. The typed layer
    /// is exercised through this without touching the process environment.
    fn raw_minimal() -> RawConfig {
        RawConfig {
            github_client_id: "id".into(),
            github_client_secret: "secret".into(),
            database_url: "postgres://localhost/db".into(),
            registry_private_key: "pem".into(),
            registry_service: default_registry_service(),
            registry_url: default_registry_url(),
            registry_public_host: default_registry_public_host(),
            host: default_host(),
            port: default_port(),
            enable_coordinator: None,
            machine_provider: default_machine_provider(),
            game_host_image: default_game_host_image(),
            agents_per_game: default_agents_per_game(),
            game_tick_rate_ms: default_tick_rate_ms(),
            game_interval_secs: default_game_interval_secs(),
            game_host_connect_timeout_secs: default_connect_timeout_secs(),
            arena_width: default_arena_dim(),
            arena_height: default_arena_dim(),
            agent_name_prefix: default_name_prefix(),
            docker_registry_pull_host: default_registry_pull_host(),
            docker_network: None,
            machine_cpus: default_machine_cpus(),
            machine_mem_mib: default_machine_mem_mib(),
            msb_host_port_base: default_msb_host_port_base(),
            msb_host_bind: default_msb_host_bind(),
            msb_registry_insecure: None,
            msb_max_duration_secs: None,
            reaper_interval_secs: default_reaper_interval_secs(),
            reaper_max_age_secs: default_reaper_max_age_secs(),
            reaper_prefix: None,
        }
    }

    #[test]
    fn coordinator_disabled_by_default() {
        let config = Config::from_raw(raw_minimal()).unwrap();
        assert!(config.coordinator.is_none());
    }

    #[test]
    fn enable_coordinator_needs_a_truthy_value() {
        let mut raw = raw_minimal();
        raw.enable_coordinator = Some("false".into());
        assert!(Config::from_raw(raw).unwrap().coordinator.is_none());

        let mut raw = raw_minimal();
        raw.enable_coordinator = Some("true".into());
        assert!(Config::from_raw(raw).unwrap().coordinator.is_some());
    }

    #[test]
    fn unknown_machine_provider_fails_fast() {
        // Rejected by figment/serde at deserialize time, not by hand-written code.
        figment::Jail::expect_with(|jail| {
            jail.set_env("GITHUB_CLIENT_ID", "id");
            jail.set_env("GITHUB_CLIENT_SECRET", "secret");
            jail.set_env("DATABASE_URL", "postgres://localhost/db");
            jail.set_env("REGISTRY_PRIVATE_KEY", "pem");
            jail.set_env("MACHINE_PROVIDER", "podman");
            assert!(matches!(Config::load(), Err(ConfigError::Extract(_))));
            Ok(())
        });
    }

    #[test]
    fn docker_requires_a_network() {
        let mut raw = raw_minimal();
        raw.enable_coordinator = Some("1".into());
        raw.machine_provider = MachineProviderKind::Docker;
        assert!(matches!(
            Config::from_raw(raw),
            Err(ConfigError::MissingDockerNetwork)
        ));
    }

    #[test]
    fn docker_backend_is_built_from_config() {
        let mut raw = raw_minimal();
        raw.enable_coordinator = Some("1".into());
        raw.machine_provider = MachineProviderKind::Docker;
        raw.docker_network = Some("gameserver_default".into());

        let coordinator = Config::from_raw(raw).unwrap().coordinator.unwrap();
        match coordinator.provider {
            ProviderConfig::Docker(cfg) => {
                assert_eq!(cfg.network, "gameserver_default");
                assert_eq!(cfg.name_prefix, "achtung-");
            }
            other => panic!("expected docker provider, got {other:?}"),
        }
    }

    #[test]
    fn microsandbox_is_the_default_backend() {
        let mut raw = raw_minimal();
        raw.enable_coordinator = Some("1".into());
        raw.msb_registry_insecure = Some("true".into());

        let coordinator = Config::from_raw(raw).unwrap().coordinator.unwrap();
        assert_eq!(coordinator.arena_width, 1000);
        assert_eq!(coordinator.arena_height, 1000);
        match coordinator.provider {
            ProviderConfig::Microsandbox(cfg) => {
                assert!(cfg.registry_insecure);
                assert_eq!(cfg.host_port_base, 51000);
            }
            other => panic!("expected microsandbox provider, got {other:?}"),
        }
    }

    #[test]
    fn env_overrides_file_and_missing_file_is_fine() {
        figment::Jail::expect_with(|jail| {
            // No config.toml yet: the environment alone must suffice.
            jail.set_env("GITHUB_CLIENT_ID", "id");
            jail.set_env("GITHUB_CLIENT_SECRET", "secret");
            jail.set_env("DATABASE_URL", "postgres://localhost/db");
            jail.set_env("REGISTRY_PRIVATE_KEY", "pem");
            let config = Config::load().expect("env-only config should load");
            assert_eq!(config.github.client_id, "id");

            // Now a file provides a base value that the environment overrides.
            jail.create_file(
                "config.toml",
                r#"
                github_client_id = "from-file"
                registry_url = "http://from-file:5001"
                "#,
            )?;
            let config = Config::load().expect("file+env config should load");
            assert_eq!(config.github.client_id, "id", "env should win over file");
            assert_eq!(config.registry.url, "http://from-file:5001");
            Ok(())
        });
    }

    #[test]
    fn reaper_prefix_defaults_to_the_name_prefix() {
        let mut raw = raw_minimal();
        raw.enable_coordinator = Some("1".into());
        raw.agent_name_prefix = "custom-".into();

        let coordinator = Config::from_raw(raw).unwrap().coordinator.unwrap();
        assert_eq!(coordinator.reaper.prefix, "custom-");
    }
}
