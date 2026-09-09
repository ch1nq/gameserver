//! Typed website startup configuration, parsed once via the `config` crate.
//!
//! Every field documents the env var it comes from so `.env.example` can be
//! generated from these docs. [`WebsiteConfig::from_env`] collects the process
//! environment into a map and delegates to [`WebsiteConfig::from_map`] so tests
//! never mutate process env.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serde::Deserialize;

use crate::env_names;
use crate::error::ConfigError;

// ─── Defaults (must match historical behavior) ───────────────────────────────

pub const DEFAULT_HOST: &str = "0.0.0.0";
pub const DEFAULT_WEBSITE_PORT: u16 = 3000;
pub const DEFAULT_RUST_LOG: &str = "website=debug,achtung-core=debug,coordinator=debug,achtung-api=debug,agent_infra=debug,axum_login=debug,tower_sessions=debug,sqlx=warn,tower_http=debug,registry-auth=debug";

pub const DEFAULT_REGISTRY_SERVICE: &str = "registry:5001";
pub const DEFAULT_REGISTRY_URL: &str = "http://localhost:5001";
pub const DEFAULT_REGISTRY_PUBLIC_HOST: &str = "localhost:5001";

pub const DEFAULT_GAME_HOST_IMAGE: &str = "ghcr.io/ch1nq/achtung-game-host:latest";
pub const DEFAULT_AGENTS_PER_GAME: usize = 4;
pub const DEFAULT_GAME_TICK_RATE_MS: u64 = 50;
pub const DEFAULT_GAME_INTERVAL_SECS: u64 = 10;
pub const DEFAULT_GAME_HOST_CONNECT_TIMEOUT_SECS: u64 = 60;

pub const DEFAULT_DOCKER_REGISTRY_PULL_HOST: &str = "localhost:5001";
pub const DEFAULT_AGENT_NAME_PREFIX: &str = "achtung-";

pub const DEFAULT_MACHINE_CPUS: u8 = 1;
pub const DEFAULT_MACHINE_MEM_MIB: u32 = 512;

pub const DEFAULT_MSB_HOST_PORT_BASE: u16 = 51000;
pub const DEFAULT_MSB_REGISTRY_INSECURE: bool = false;

pub const DEFAULT_REAPER_INTERVAL_SECS: u64 = 300;
pub const DEFAULT_REAPER_MAX_AGE_SECS: u64 = 3600;

fn default_host() -> String {
    DEFAULT_HOST.to_string()
}
fn default_website_port() -> u16 {
    DEFAULT_WEBSITE_PORT
}
fn default_rust_log() -> String {
    DEFAULT_RUST_LOG.to_string()
}
fn default_registry_service() -> String {
    DEFAULT_REGISTRY_SERVICE.to_string()
}
fn default_registry_url() -> String {
    DEFAULT_REGISTRY_URL.to_string()
}
fn default_registry_public_host() -> String {
    DEFAULT_REGISTRY_PUBLIC_HOST.to_string()
}
fn default_game_host_image() -> String {
    DEFAULT_GAME_HOST_IMAGE.to_string()
}
fn default_agents_per_game() -> usize {
    DEFAULT_AGENTS_PER_GAME
}
fn default_tick_rate_ms() -> u64 {
    DEFAULT_GAME_TICK_RATE_MS
}
fn default_game_interval_secs() -> u64 {
    DEFAULT_GAME_INTERVAL_SECS
}
fn default_connect_timeout_secs() -> u64 {
    DEFAULT_GAME_HOST_CONNECT_TIMEOUT_SECS
}
fn default_registry_pull_host() -> String {
    DEFAULT_DOCKER_REGISTRY_PULL_HOST.to_string()
}
fn default_name_prefix() -> String {
    DEFAULT_AGENT_NAME_PREFIX.to_string()
}
fn default_cpus() -> u8 {
    DEFAULT_MACHINE_CPUS
}
fn default_mem_mib() -> u32 {
    DEFAULT_MACHINE_MEM_MIB
}
fn default_host_port_base() -> u16 {
    DEFAULT_MSB_HOST_PORT_BASE
}
fn default_host_bind() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}
fn default_registry_insecure() -> bool {
    DEFAULT_MSB_REGISTRY_INSECURE
}
fn default_reaper_interval() -> u64 {
    DEFAULT_REAPER_INTERVAL_SECS
}
fn default_reaper_max_age() -> u64 {
    DEFAULT_REAPER_MAX_AGE_SECS
}
fn default_provider() -> MachineProviderKind {
    MachineProviderKind::Microsandbox
}

// ─── Public typed config ─────────────────────────────────────────────────────

/// Which machine backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MachineProviderKind {
    Docker,
    Microsandbox,
}

impl std::fmt::Display for MachineProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Docker => write!(f, "docker"),
            Self::Microsandbox => write!(f, "microsandbox"),
        }
    }
}

/// Website listen address (`HOST` / `PORT`) plus log filter (`RUST_LOG`).
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Bind host. Env: `HOST` (default `0.0.0.0`).
    #[serde(default = "default_host")]
    pub host: String,
    /// Bind port. Env: `PORT` (default `3000`).
    #[serde(default = "default_website_port")]
    pub port: u16,
    /// Tracing filter. Env: `RUST_LOG` (default: per-crate debug).
    #[serde(default = "default_rust_log")]
    pub rust_log: String,
}

impl ServerConfig {
    pub fn addr(&self) -> Result<SocketAddr, ConfigError> {
        format!("{}:{}", self.host, self.port).parse().map_err(|e| {
            ConfigError::invalid(
                env_names::PORT,
                self.port.to_string(),
                format!("invalid socket addr: {e}"),
            )
        })
    }
}

/// GitHub OAuth credentials (both required).
#[derive(Debug, Clone, Deserialize)]
pub struct GithubConfig {
    /// Env: `GITHUB_CLIENT_ID` (required).
    pub client_id: String,
    /// Env: `GITHUB_CLIENT_SECRET` (required).
    pub client_secret: String,
}

/// Database connection (required).
#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    /// Env: `DATABASE_URL` (required).
    pub url: String,
}

/// Registry addressing + signing key.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryConfig {
    /// RSA private key (PEM) for minting registry JWTs. Env: `REGISTRY_PRIVATE_KEY` (required).
    pub private_key_pem: String,
    /// JWT audience; must equal the registry's `REGISTRY_AUTH_TOKEN_SERVICE`. Env: `REGISTRY_SERVICE` (default `registry:5001`).
    #[serde(default = "default_registry_service")]
    pub service: String,
    /// How the website reaches the registry API. Env: `REGISTRY_URL` (default `http://localhost:5001`).
    #[serde(default = "default_registry_url")]
    pub url: String,
    /// Host rendered into user-facing docker hints. Env: `REGISTRY_PUBLIC_HOST` (default `localhost:5001`).
    #[serde(default = "default_registry_public_host")]
    pub public_host: String,
}

/// Game pacing + host image.
#[derive(Debug, Clone, Deserialize)]
pub struct GameSection {
    /// Env: `GAME_HOST_IMAGE` (default `ghcr.io/ch1nq/achtung-game-host:latest`).
    #[serde(default = "default_game_host_image")]
    pub host_image: String,
    /// Env: `AGENTS_PER_GAME` (default `4`, must be > 0).
    #[serde(default = "default_agents_per_game")]
    pub agents_per_game: usize,
    /// Env: `GAME_TICK_RATE_MS` (default `50`, must be > 0).
    #[serde(default = "default_tick_rate_ms")]
    pub tick_rate_ms: u64,
    /// Env: `GAME_INTERVAL_SECS` (default `10`).
    #[serde(default = "default_game_interval_secs")]
    pub interval_secs: u64,
    /// Env: `GAME_HOST_CONNECT_TIMEOUT_SECS` (default `60`).
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
}

/// Docker backend (`MACHINE_PROVIDER=docker`).
#[derive(Debug, Clone, Deserialize)]
pub struct DockerSection {
    /// Shared network the website/coordinator container is on. Env: `DOCKER_NETWORK` (required for docker).
    pub network: Option<String>,
    /// Registry host the Docker daemon pulls from. Env: `DOCKER_REGISTRY_PULL_HOST` (default `localhost:5001`).
    #[serde(default = "default_registry_pull_host")]
    pub registry_pull_host: String,
    /// Prefix for container names. Env: `AGENT_NAME_PREFIX` (default `achtung-`).
    #[serde(default = "default_name_prefix")]
    pub name_prefix: String,
}

/// microsandbox backend (`MACHINE_PROVIDER=microsandbox`, the default).
#[derive(Debug, Clone, Deserialize)]
pub struct MicrosandboxSection {
    /// vCPU limit per sandbox. Env: `MACHINE_CPUS` (default `1`).
    #[serde(default = "default_cpus")]
    pub cpus: u8,
    /// Memory limit per sandbox in MiB. Env: `MACHINE_MEM_MIB` (default `512`).
    #[serde(default = "default_mem_mib")]
    pub memory_mib: u32,
    /// First host port of the relay range. Env: `MSB_HOST_PORT_BASE` (default `51000`).
    #[serde(default = "default_host_port_base")]
    pub host_port_base: u16,
    /// Host address agent relay ports bind to. Env: `MSB_HOST_BIND` (default `127.0.0.1`).
    #[serde(default = "default_host_bind")]
    pub host_bind: IpAddr,
    /// Registry host prefixed onto private image refs. Env: `DOCKER_REGISTRY_PULL_HOST` (default `localhost:5001`).
    #[serde(default = "default_registry_pull_host")]
    pub registry_pull_host: String,
    /// Pull over plain HTTP (local dev only). Env: `MSB_REGISTRY_INSECURE` (default `false`).
    #[serde(default = "default_registry_insecure")]
    pub registry_insecure: bool,
    /// Hard per-sandbox lifetime cap in seconds. Env: `MSB_MAX_DURATION_SECS` (optional, unset = no cap).
    #[serde(default)]
    pub max_duration_secs: Option<u64>,
}

/// Orphan-cleanup reaper.
#[derive(Debug, Clone, Deserialize)]
pub struct ReaperSection {
    /// Env: `REAPER_INTERVAL_SECS` (default `300`).
    #[serde(default = "default_reaper_interval")]
    pub interval_secs: u64,
    /// Env: `REAPER_MAX_AGE_SECS` (default `3600`).
    #[serde(default = "default_reaper_max_age")]
    pub max_age_secs: u64,
    /// Env: `REAPER_PREFIX` (default: `AGENT_NAME_PREFIX`).
    pub prefix: Option<String>,
}

/// Coordinator (present iff `ENABLE_COORDINATOR` is truthy).
#[derive(Debug, Clone, Deserialize)]
pub struct CoordinatorConfig {
    /// Env: `MACHINE_PROVIDER` (default `microsandbox`).
    #[serde(default = "default_provider")]
    pub provider: MachineProviderKind,
    #[serde(default = "default_game_section")]
    pub game: GameSection,
    #[serde(default)]
    pub docker: DockerSectionOpt,
    #[serde(default)]
    pub microsandbox: MicrosandboxSectionOpt,
    #[serde(default = "default_reaper_section")]
    pub reaper: ReaperSectionRaw,
}

// Raw wrappers so missing subsections still deserialize (defaults apply),
// while required-within-backend fields (docker.network) stay Option-checked.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DockerSectionOpt {
    pub network: Option<String>,
    #[serde(default = "default_registry_pull_host")]
    pub registry_pull_host: String,
    #[serde(default = "default_name_prefix")]
    pub name_prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MicrosandboxSectionOpt {
    #[serde(default = "default_cpus")]
    pub cpus: u8,
    #[serde(default = "default_mem_mib")]
    pub memory_mib: u32,
    #[serde(default = "default_host_port_base")]
    pub host_port_base: u16,
    #[serde(default = "default_host_bind")]
    pub host_bind: IpAddr,
    #[serde(default = "default_registry_pull_host")]
    pub registry_pull_host: String,
    #[serde(default = "default_registry_insecure")]
    pub registry_insecure: bool,
    #[serde(default)]
    pub max_duration_secs: Option<u64>,
}

impl Default for MicrosandboxSectionOpt {
    fn default() -> Self {
        Self {
            cpus: default_cpus(),
            memory_mib: default_mem_mib(),
            host_port_base: default_host_port_base(),
            host_bind: default_host_bind(),
            registry_pull_host: default_registry_pull_host(),
            registry_insecure: default_registry_insecure(),
            max_duration_secs: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReaperSectionRaw {
    #[serde(default = "default_reaper_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_reaper_max_age")]
    pub max_age_secs: u64,
    #[serde(default)]
    pub prefix: Option<String>,
}

fn default_game_section() -> GameSection {
    GameSection {
        host_image: default_game_host_image(),
        agents_per_game: default_agents_per_game(),
        tick_rate_ms: default_tick_rate_ms(),
        interval_secs: default_game_interval_secs(),
        connect_timeout_secs: default_connect_timeout_secs(),
    }
}

fn default_reaper_section() -> ReaperSectionRaw {
    ReaperSectionRaw {
        interval_secs: default_reaper_interval(),
        max_age_secs: default_reaper_max_age(),
        prefix: None,
    }
}

// Top-level raw shape matching the dotted keys we feed the `config` crate.
#[derive(Debug, Deserialize)]
struct RawWebsite {
    #[serde(default = "default_raw_server")]
    server: ServerConfig,
    github: Option<RawGithub>,
    db: Option<RawDb>,
    registry: Option<RawRegistry>,
    coordinator: Option<CoordinatorConfig>,
    /// Top-level agent prefix (shared default source for docker + reaper).
    #[serde(default = "default_name_prefix")]
    agent_name_prefix: String,
}

fn default_raw_server() -> ServerConfig {
    ServerConfig {
        host: default_host(),
        port: default_website_port(),
        rust_log: default_rust_log(),
    }
}

#[derive(Debug, Deserialize)]
struct RawGithub {
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDb {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawRegistry {
    private_key_pem: Option<String>,
    #[serde(default = "default_registry_service")]
    service: String,
    #[serde(default = "default_registry_url")]
    url: String,
    #[serde(default = "default_registry_public_host")]
    public_host: String,
}

/// Fully validated website configuration.
#[derive(Debug, Clone)]
pub struct WebsiteConfig {
    pub server: ServerConfig,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub database_url: String,
    pub registry_private_key_pem: String,
    pub registry_service: String,
    pub registry_url: String,
    pub registry_public_host: String,
    pub rust_log: String,
    /// `None` when `ENABLE_COORDINATOR` is unset/falsy.
    pub coordinator: Option<ResolvedCoordinator>,
}

/// Coordinator with backend-specific section resolved + validated.
#[derive(Debug, Clone)]
pub struct ResolvedCoordinator {
    pub provider: MachineProviderKind,
    pub game_host_image: String,
    pub agents_per_game: usize,
    pub tick_rate_ms: u64,
    pub game_interval_secs: u64,
    pub game_host_connect_timeout_secs: u64,
    pub game_host_grpc_port: u16,
    pub agent_grpc_port: u16,
    pub docker_network: Option<String>,
    pub registry_pull_host: String,
    pub agent_name_prefix: String,
    pub machine_cpus: u8,
    pub machine_memory_mib: u32,
    pub msb_host_port_base: u16,
    pub msb_host_bind: IpAddr,
    pub msb_registry_insecure: bool,
    pub msb_max_duration_secs: Option<u64>,
    pub reaper_interval_secs: u64,
    pub reaper_max_age_secs: u64,
    pub reaper_prefix: String,
}

impl WebsiteConfig {
    /// Parse from the process environment (fail-fast, single error).
    pub fn from_env() -> Result<Self, ConfigError> {
        let map: HashMap<String, String> = std::env::vars().collect();
        Self::from_map(map)
    }

    /// Parse from an explicit map (tests use this; no process-env mutation).
    ///
    /// Uses the `config` crate for layered defaults + typed deserialization,
    /// then applies fail-fast validation so unknown `MACHINE_PROVIDER` etc.
    /// surface before any `TcpListener`/DB work.
    pub fn from_map(mut map: HashMap<String, String>) -> Result<Self, ConfigError> {
        // Bare presence (empty string) counts as `true` for back-compat with
        // the old `env::var("ENABLE_COORDINATOR").is_ok()` check. The `config`
        // crate already parses 1/0/yes/no/on/off case-insensitively.
        if let Some(v) = map.get(env_names::ENABLE_COORDINATOR)
            && v.trim().is_empty()
        {
            map.insert(
                env_names::ENABLE_COORDINATOR.to_string(),
                "true".to_string(),
            );
        }

        let enable_coordinator_raw = map.get(env_names::ENABLE_COORDINATOR).cloned();
        let coordinator_enabled = match enable_coordinator_raw.as_deref() {
            None => false,
            Some(v) => parse_bool_lenient(v).map_err(|_| {
                ConfigError::invalid(
                    env_names::ENABLE_COORDINATOR,
                    v.to_string(),
                    "expected a boolean (1/true/yes/on or 0/false/no/off; empty means true)",
                )
            })?,
        };

        let mut b = config::Config::builder();
        // Server defaults.
        b = b
            .set_default("server.host", DEFAULT_HOST)
            .map_err(ConfigError::from)?;
        b = b
            .set_default("server.port", i64::from(DEFAULT_WEBSITE_PORT))
            .map_err(ConfigError::from)?;
        b = b
            .set_default("server.rust_log", DEFAULT_RUST_LOG)
            .map_err(ConfigError::from)?;
        b = b
            .set_default("agent_name_prefix", DEFAULT_AGENT_NAME_PREFIX)
            .map_err(ConfigError::from)?;
        // Coordinator defaults (only materialize when enabled).
        b = b
            .set_default("coordinator.provider", "microsandbox")
            .map_err(ConfigError::from)?;
        b = b
            .set_default("coordinator.game.host_image", DEFAULT_GAME_HOST_IMAGE)
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.game.agents_per_game",
                DEFAULT_AGENTS_PER_GAME as i64,
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.game.tick_rate_ms",
                DEFAULT_GAME_TICK_RATE_MS as i64,
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.game.interval_secs",
                DEFAULT_GAME_INTERVAL_SECS as i64,
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.game.connect_timeout_secs",
                DEFAULT_GAME_HOST_CONNECT_TIMEOUT_SECS as i64,
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.docker.registry_pull_host",
                DEFAULT_DOCKER_REGISTRY_PULL_HOST,
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default("coordinator.docker.name_prefix", DEFAULT_AGENT_NAME_PREFIX)
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.microsandbox.cpus",
                i64::from(DEFAULT_MACHINE_CPUS),
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.microsandbox.memory_mib",
                i64::from(DEFAULT_MACHINE_MEM_MIB),
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.microsandbox.host_port_base",
                i64::from(DEFAULT_MSB_HOST_PORT_BASE),
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default("coordinator.microsandbox.host_bind", "127.0.0.1")
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.microsandbox.registry_pull_host",
                DEFAULT_DOCKER_REGISTRY_PULL_HOST,
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.microsandbox.registry_insecure",
                DEFAULT_MSB_REGISTRY_INSECURE,
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.reaper.interval_secs",
                DEFAULT_REAPER_INTERVAL_SECS as i64,
            )
            .map_err(ConfigError::from)?;
        b = b
            .set_default(
                "coordinator.reaper.max_age_secs",
                DEFAULT_REAPER_MAX_AGE_SECS as i64,
            )
            .map_err(ConfigError::from)?;

        // Flat env → nested keys.
        let overrides: &[(&str, &str)] = &[
            ("server.host", env_names::HOST),
            ("server.port", env_names::PORT),
            ("server.rust_log", env_names::RUST_LOG),
            ("github.client_id", env_names::GITHUB_CLIENT_ID),
            ("github.client_secret", env_names::GITHUB_CLIENT_SECRET),
            ("db.url", env_names::DATABASE_URL),
            ("registry.private_key_pem", env_names::REGISTRY_PRIVATE_KEY),
            ("registry.service", env_names::REGISTRY_SERVICE),
            ("registry.url", env_names::REGISTRY_URL),
            ("registry.public_host", env_names::REGISTRY_PUBLIC_HOST),
            ("agent_name_prefix", env_names::AGENT_NAME_PREFIX),
            ("coordinator.provider", env_names::MACHINE_PROVIDER),
            ("coordinator.game.host_image", env_names::GAME_HOST_IMAGE),
            (
                "coordinator.game.agents_per_game",
                env_names::AGENTS_PER_GAME,
            ),
            (
                "coordinator.game.tick_rate_ms",
                env_names::GAME_TICK_RATE_MS,
            ),
            (
                "coordinator.game.interval_secs",
                env_names::GAME_INTERVAL_SECS,
            ),
            (
                "coordinator.game.connect_timeout_secs",
                env_names::GAME_HOST_CONNECT_TIMEOUT_SECS,
            ),
            ("coordinator.docker.network", env_names::DOCKER_NETWORK),
            (
                "coordinator.docker.registry_pull_host",
                env_names::DOCKER_REGISTRY_PULL_HOST,
            ),
            (
                "coordinator.docker.name_prefix",
                env_names::AGENT_NAME_PREFIX,
            ),
            ("coordinator.microsandbox.cpus", env_names::MACHINE_CPUS),
            (
                "coordinator.microsandbox.memory_mib",
                env_names::MACHINE_MEM_MIB,
            ),
            (
                "coordinator.microsandbox.host_port_base",
                env_names::MSB_HOST_PORT_BASE,
            ),
            (
                "coordinator.microsandbox.host_bind",
                env_names::MSB_HOST_BIND,
            ),
            (
                "coordinator.microsandbox.registry_pull_host",
                env_names::DOCKER_REGISTRY_PULL_HOST,
            ),
            (
                "coordinator.microsandbox.registry_insecure",
                env_names::MSB_REGISTRY_INSECURE,
            ),
            (
                "coordinator.microsandbox.max_duration_secs",
                env_names::MSB_MAX_DURATION_SECS,
            ),
            (
                "coordinator.reaper.interval_secs",
                env_names::REAPER_INTERVAL_SECS,
            ),
            (
                "coordinator.reaper.max_age_secs",
                env_names::REAPER_MAX_AGE_SECS,
            ),
            ("coordinator.reaper.prefix", env_names::REAPER_PREFIX),
        ];
        for (nested, flat) in overrides.iter().copied() {
            if let Some(v) = map.get(flat) {
                // Skip empty-string overrides for optional numeric/bool fields so
                // an explicitly empty var falls back to its default instead of
                // a coercion error — except required strings and the coordinator
                // toggle, which are handled explicitly.
                if v.is_empty()
                    && !matches!(
                        flat,
                        env_names::GITHUB_CLIENT_ID
                            | env_names::GITHUB_CLIENT_SECRET
                            | env_names::DATABASE_URL
                            | env_names::REGISTRY_PRIVATE_KEY
                            | env_names::DOCKER_NETWORK
                            | env_names::GAME_HOST_IMAGE
                            | env_names::ENABLE_COORDINATOR
                    )
                {
                    continue;
                }
                b = b
                    .set_override(nested, v.clone())
                    .map_err(|e| ConfigError::invalid(flat, v.clone(), e.to_string()))?;
            }
        }

        let built = b.build().map_err(ConfigError::from)?;
        let raw: RawWebsite = built
            .try_deserialize()
            .map_err(|e| map_config_error(e, &map))?;

        // Required top-level fields → precise Missing errors.
        let github = raw.github.unwrap_or(RawGithub {
            client_id: None,
            client_secret: None,
        });
        let client_id = non_empty_opt(
            github.client_id,
            env_names::GITHUB_CLIENT_ID,
            "GitHub OAuth app client id",
        )?;
        let client_secret = non_empty_opt(
            github.client_secret,
            env_names::GITHUB_CLIENT_SECRET,
            "GitHub OAuth app client secret",
        )?;
        let db_url = non_empty_opt(
            raw.db.and_then(|d| d.url),
            env_names::DATABASE_URL,
            "Postgres connection string, e.g. postgresql://arcadio:arcadio@localhost:5432/arcadio",
        )?;
        let registry = raw.registry.unwrap_or(RawRegistry {
            private_key_pem: None,
            service: default_registry_service(),
            url: default_registry_url(),
            public_host: default_registry_public_host(),
        });
        let private_key_pem = non_empty_opt(
            registry.private_key_pem,
            env_names::REGISTRY_PRIVATE_KEY,
            "RSA private key (PEM) used to mint registry JWTs",
        )?;

        if raw.server.host.trim().is_empty() {
            return Err(ConfigError::invalid(
                env_names::HOST,
                raw.server.host,
                "must not be empty",
            ));
        }
        if raw.server.port == 0 {
            return Err(ConfigError::invalid(
                env_names::PORT,
                "0",
                "must be a non-zero port",
            ));
        }

        let coordinator = if coordinator_enabled {
            let c = raw.coordinator.ok_or_else(|| {
                ConfigError::conflict(
                    env_names::ENABLE_COORDINATOR,
                    "coordinator enabled but coordinator section failed to build",
                )
            })?;
            Some(resolve_coordinator(c, &raw.agent_name_prefix)?)
        } else {
            None
        };

        Ok(Self {
            server: raw.server.clone(),
            github_client_id: client_id,
            github_client_secret: client_secret,
            database_url: db_url,
            registry_private_key_pem: private_key_pem,
            registry_service: registry.service,
            registry_url: registry.url,
            registry_public_host: registry.public_host,
            rust_log: raw.server.rust_log.clone(),
            coordinator,
        })
    }
}

fn resolve_coordinator(
    c: CoordinatorConfig,
    agent_name_prefix: &str,
) -> Result<ResolvedCoordinator, ConfigError> {
    if c.game.agents_per_game == 0 {
        return Err(ConfigError::invalid(
            env_names::AGENTS_PER_GAME,
            "0",
            "must be at least 1",
        ));
    }
    if c.game.tick_rate_ms == 0 {
        return Err(ConfigError::invalid(
            env_names::GAME_TICK_RATE_MS,
            "0",
            "must be at least 1 ms",
        ));
    }
    if c.game.host_image.trim().is_empty() {
        return Err(ConfigError::missing(
            env_names::GAME_HOST_IMAGE,
            "game host image, e.g. ghcr.io/ch1nq/achtung-game-host:latest",
        ));
    }
    // Validate via the shared ImageUrl type (currently non-empty check).
    if let Err(e) = common::ImageUrl::new(c.game.host_image.clone()) {
        return Err(ConfigError::invalid(
            env_names::GAME_HOST_IMAGE,
            c.game.host_image.clone(),
            e.to_string(),
        ));
    }

    // Backend-specific required fields.
    let docker_network = match c.provider {
        MachineProviderKind::Docker => {
            let net = c.docker.network.clone().unwrap_or_default();
            if net.trim().is_empty() {
                return Err(ConfigError::missing(
                    env_names::DOCKER_NETWORK,
                    "required when MACHINE_PROVIDER=docker (the shared network the website is on)",
                ));
            }
            Some(net)
        }
        MachineProviderKind::Microsandbox => c.docker.network.clone(),
    };

    // Reaper prefix defaults to the agent name prefix (single source so naming
    // and reaping cannot drift).
    let agent_prefix = if c.docker.name_prefix.trim().is_empty() {
        agent_name_prefix.to_string()
    } else {
        c.docker.name_prefix.clone()
    };
    let reaper_prefix = c
        .reaper
        .prefix
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| agent_prefix.clone());

    // Registry pull host is shared by both backends; prefer the explicitly set
    // one (they alias the same env var, so they agree unless defaults differ).
    let registry_pull_host = if c.docker.registry_pull_host.trim().is_empty() {
        c.microsandbox.registry_pull_host.clone()
    } else {
        c.docker.registry_pull_host.clone()
    };

    Ok(ResolvedCoordinator {
        provider: c.provider,
        game_host_image: c.game.host_image,
        agents_per_game: c.game.agents_per_game,
        tick_rate_ms: c.game.tick_rate_ms,
        game_interval_secs: c.game.interval_secs,
        game_host_connect_timeout_secs: c.game.connect_timeout_secs,
        game_host_grpc_port: env_names::DEFAULT_GAME_HOST_GRPC_PORT,
        agent_grpc_port: env_names::DEFAULT_AGENT_GRPC_PORT,
        docker_network,
        registry_pull_host,
        agent_name_prefix: agent_prefix,
        machine_cpus: c.microsandbox.cpus,
        machine_memory_mib: c.microsandbox.memory_mib,
        msb_host_port_base: c.microsandbox.host_port_base,
        msb_host_bind: c.microsandbox.host_bind,
        msb_registry_insecure: c.microsandbox.registry_insecure,
        msb_max_duration_secs: c.microsandbox.max_duration_secs,
        reaper_interval_secs: c.reaper.interval_secs,
        reaper_max_age_secs: c.reaper.max_age_secs,
        reaper_prefix,
    })
}

fn non_empty_opt(v: Option<String>, key: &'static str, hint: &str) -> Result<String, ConfigError> {
    match v {
        Some(s) if !s.trim().is_empty() => Ok(s),
        _ => Err(ConfigError::missing(key, hint)),
    }
}

/// Lenient bool: accepts config-crate's set plus empty (handled by caller).
fn parse_bool_lenient(v: &str) -> Result<bool, ()> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        "" => Ok(true),
        _ => Err(()),
    }
}

/// Translate a `config::ConfigError` into the most helpful `ConfigError` by
/// sniffing the dotted key path back to its flat env var name.
fn map_config_error(e: config::ConfigError, map: &HashMap<String, String>) -> ConfigError {
    let msg = e.to_string();
    // Order matters: more specific paths first.
    let candidates: &[(&str, &'static str)] = &[
        ("coordinator.provider", env_names::MACHINE_PROVIDER),
        ("coordinator.docker.network", env_names::DOCKER_NETWORK),
        ("coordinator.game.host_image", env_names::GAME_HOST_IMAGE),
        (
            "coordinator.game.agents_per_game",
            env_names::AGENTS_PER_GAME,
        ),
        (
            "coordinator.game.tick_rate_ms",
            env_names::GAME_TICK_RATE_MS,
        ),
        (
            "coordinator.game.interval_secs",
            env_names::GAME_INTERVAL_SECS,
        ),
        (
            "coordinator.game.connect_timeout_secs",
            env_names::GAME_HOST_CONNECT_TIMEOUT_SECS,
        ),
        ("coordinator.microsandbox.cpus", env_names::MACHINE_CPUS),
        (
            "coordinator.microsandbox.memory_mib",
            env_names::MACHINE_MEM_MIB,
        ),
        (
            "coordinator.microsandbox.host_port_base",
            env_names::MSB_HOST_PORT_BASE,
        ),
        (
            "coordinator.microsandbox.host_bind",
            env_names::MSB_HOST_BIND,
        ),
        (
            "coordinator.microsandbox.registry_insecure",
            env_names::MSB_REGISTRY_INSECURE,
        ),
        (
            "coordinator.microsandbox.max_duration_secs",
            env_names::MSB_MAX_DURATION_SECS,
        ),
        (
            "coordinator.reaper.interval_secs",
            env_names::REAPER_INTERVAL_SECS,
        ),
        (
            "coordinator.reaper.max_age_secs",
            env_names::REAPER_MAX_AGE_SECS,
        ),
        ("server.port", env_names::PORT),
        ("server.host", env_names::HOST),
    ];
    for (path, flat) in candidates.iter().copied() {
        if msg.contains(path) {
            let val = map.get(flat).cloned().unwrap_or_default();
            let reason = if flat == env_names::MACHINE_PROVIDER {
                format!("{msg} (expected \"docker\" or \"microsandbox\")")
            } else {
                msg
            };
            return ConfigError::invalid(flat, val, reason);
        }
    }
    ConfigError::Config(e)
}
