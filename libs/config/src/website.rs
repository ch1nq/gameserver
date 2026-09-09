//! Typed website startup configuration, parsed once via the `config` crate.
//!
//! Loading is declarative: a flat [`RawWebsite`] mirrors the process
//! environment (the crate lowercases keys, so `database_url` binds
//! `DATABASE_URL`), and a single [`config::Environment`] source provides
//! parsing (`"50"` → `50`), empty-dropping, and test injection. Semantic
//! validation lives in [`RawWebsite::resolve`]; every field documents the env
//! var it comes from so `.env.example` can be generated from these docs.
//! [`WebsiteConfig::from_env`] collects the process environment and delegates
//! to [`WebsiteConfig::from_map`] so tests never mutate process env.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serde::Deserialize;

use crate::env_names::{self, ALL_ENV_VARS};
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
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Bind host. Env: `HOST` (default `0.0.0.0`).
    pub host: String,
    /// Bind port. Env: `PORT` (default `3000`).
    pub port: u16,
    /// Tracing filter. Env: `RUST_LOG` (default: per-crate debug).
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

// ─── Flat raw shape: one field per env var ───────────────────────────────────
//
// `config::Environment` lowercases keys, so snake_case fields bind their
// `SCREAMING` vars with no mapping table.

/// Flat deserialization target; every field documents its env var.
#[derive(Debug, Deserialize)]
struct RawWebsite {
    /// Env: `HOST` (default `0.0.0.0`).
    #[serde(default = "default_host")]
    host: String,
    /// Env: `PORT` (default `3000`).
    #[serde(default = "default_website_port")]
    port: u16,
    /// Env: `RUST_LOG` (default: per-crate debug).
    #[serde(default = "default_rust_log")]
    rust_log: String,
    /// Env: `GITHUB_CLIENT_ID` (required).
    github_client_id: Option<String>,
    /// Env: `GITHUB_CLIENT_SECRET` (required).
    github_client_secret: Option<String>,
    /// Env: `DATABASE_URL` (required).
    database_url: Option<String>,
    /// Env: `REGISTRY_PRIVATE_KEY` (required).
    registry_private_key: Option<String>,
    /// Env: `REGISTRY_SERVICE` (default `registry:5001`).
    #[serde(default = "default_registry_service")]
    registry_service: String,
    /// Env: `REGISTRY_URL` (default `http://localhost:5001`).
    #[serde(default = "default_registry_url")]
    registry_url: String,
    /// Env: `REGISTRY_PUBLIC_HOST` (default `localhost:5001`).
    #[serde(default = "default_registry_public_host")]
    registry_public_host: String,
    /// Env: `ENABLE_COORDINATOR` (truthy enables; bare presence counts as true).
    #[serde(default)]
    enable_coordinator: bool,
    /// Env: `MACHINE_PROVIDER` (default `microsandbox`).
    #[serde(default = "default_provider")]
    machine_provider: MachineProviderKind,
    /// Env: `GAME_HOST_IMAGE` (default `ghcr.io/ch1nq/achtung-game-host:latest`).
    #[serde(default = "default_game_host_image")]
    game_host_image: String,
    /// Env: `AGENTS_PER_GAME` (default `4`, must be > 0).
    #[serde(default = "default_agents_per_game")]
    agents_per_game: usize,
    /// Env: `GAME_TICK_RATE_MS` (default `50`, must be > 0).
    #[serde(default = "default_tick_rate_ms")]
    game_tick_rate_ms: u64,
    /// Env: `GAME_INTERVAL_SECS` (default `10`).
    #[serde(default = "default_game_interval_secs")]
    game_interval_secs: u64,
    /// Env: `GAME_HOST_CONNECT_TIMEOUT_SECS` (default `60`).
    #[serde(default = "default_connect_timeout_secs")]
    game_host_connect_timeout_secs: u64,
    /// Env: `DOCKER_NETWORK` (required when `MACHINE_PROVIDER=docker`).
    #[serde(default)]
    docker_network: Option<String>,
    /// Env: `DOCKER_REGISTRY_PULL_HOST` (default `localhost:5001`).
    #[serde(default = "default_registry_pull_host")]
    docker_registry_pull_host: String,
    /// Env: `AGENT_NAME_PREFIX` (default `achtung-`).
    #[serde(default = "default_name_prefix")]
    agent_name_prefix: String,
    /// Env: `MACHINE_CPUS` (default `1`).
    #[serde(default = "default_cpus")]
    machine_cpus: u8,
    /// Env: `MACHINE_MEM_MIB` (default `512`).
    #[serde(default = "default_mem_mib")]
    machine_mem_mib: u32,
    /// Env: `MSB_HOST_PORT_BASE` (default `51000`).
    #[serde(default = "default_host_port_base")]
    msb_host_port_base: u16,
    /// Env: `MSB_HOST_BIND` (default `127.0.0.1`).
    #[serde(default = "default_host_bind")]
    msb_host_bind: IpAddr,
    /// Env: `MSB_REGISTRY_INSECURE` (default `false`).
    #[serde(default = "default_registry_insecure")]
    msb_registry_insecure: bool,
    /// Env: `MSB_MAX_DURATION_SECS` (optional, unset = no cap).
    #[serde(default)]
    msb_max_duration_secs: Option<u64>,
    /// Env: `REAPER_INTERVAL_SECS` (default `300`).
    #[serde(default = "default_reaper_interval")]
    reaper_interval_secs: u64,
    /// Env: `REAPER_MAX_AGE_SECS` (default `3600`).
    #[serde(default = "default_reaper_max_age")]
    reaper_max_age_secs: u64,
    /// Env: `REAPER_PREFIX` (default: `AGENT_NAME_PREFIX`).
    #[serde(default)]
    reaper_prefix: Option<String>,
}

impl WebsiteConfig {
    /// Parse from the process environment (fail-fast, single error).
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_map(std::env::vars().collect())
    }

    /// Parse from an explicit map (tests use this; no process-env mutation).
    pub fn from_map(mut map: HashMap<String, String>) -> Result<Self, ConfigError> {
        // Bare presence (empty string) counts as `true` for back-compat with
        // the old `env::var("ENABLE_COORDINATOR").is_ok()` check;
        // `ignore_empty` below would otherwise drop it (→ disabled).
        if let Some(v) = map.get(env_names::ENABLE_COORDINATOR)
            && v.trim().is_empty()
        {
            map.insert(
                env_names::ENABLE_COORDINATOR.to_string(),
                "true".to_string(),
            );
        }
        let snapshot = map.clone();
        let raw: RawWebsite = config::Config::builder()
            .add_source(
                config::Environment::default()
                    .try_parsing(true)
                    .ignore_empty(true)
                    .source(Some(map)),
            )
            .build()
            .map_err(ConfigError::from)?
            .try_deserialize()
            .map_err(|e| map_serde_error(e, &snapshot))?;
        raw.resolve()
    }
}

impl RawWebsite {
    /// Semantic validation: required non-empty strings, ranges, and the
    /// docker-backend requirement. Everything the derive layer cannot express.
    fn resolve(self) -> Result<WebsiteConfig, ConfigError> {
        // Borrow first: coordinator resolution only reads; the moves below
        // follow after the borrow ends.
        let coordinator = self.resolve_coordinator()?;
        let github_client_id = non_empty_opt(
            self.github_client_id,
            env_names::GITHUB_CLIENT_ID,
            "GitHub OAuth app client id",
        )?;
        let github_client_secret = non_empty_opt(
            self.github_client_secret,
            env_names::GITHUB_CLIENT_SECRET,
            "GitHub OAuth app client secret",
        )?;
        let database_url = non_empty_opt(
            self.database_url,
            env_names::DATABASE_URL,
            "Postgres connection string, e.g. postgresql://arcadio:arcadio@localhost:5432/arcadio",
        )?;
        let registry_private_key_pem = non_empty_opt(
            self.registry_private_key,
            env_names::REGISTRY_PRIVATE_KEY,
            "RSA private key (PEM) used to mint registry JWTs",
        )?;
        if self.host.trim().is_empty() {
            return Err(ConfigError::invalid(
                env_names::HOST,
                self.host,
                "must not be empty",
            ));
        }
        if self.port == 0 {
            return Err(ConfigError::invalid(
                env_names::PORT,
                "0",
                "must be a non-zero port",
            ));
        }

        Ok(WebsiteConfig {
            server: ServerConfig {
                host: self.host,
                port: self.port,
                rust_log: self.rust_log.clone(),
            },
            github_client_id,
            github_client_secret,
            database_url,
            registry_private_key_pem,
            registry_service: self.registry_service,
            registry_url: self.registry_url,
            registry_public_host: self.registry_public_host,
            rust_log: self.rust_log,
            coordinator,
        })
    }

    fn resolve_coordinator(&self) -> Result<Option<ResolvedCoordinator>, ConfigError> {
        if !self.enable_coordinator {
            return Ok(None);
        }
        if self.agents_per_game == 0 {
            return Err(ConfigError::invalid(
                env_names::AGENTS_PER_GAME,
                "0",
                "must be at least 1",
            ));
        }
        if self.game_tick_rate_ms == 0 {
            return Err(ConfigError::invalid(
                env_names::GAME_TICK_RATE_MS,
                "0",
                "must be at least 1 ms",
            ));
        }
        if self.game_host_image.trim().is_empty() {
            return Err(ConfigError::missing(
                env_names::GAME_HOST_IMAGE,
                "game host image, e.g. ghcr.io/ch1nq/achtung-game-host:latest",
            ));
        }
        // Validated via the shared ImageUrl type (currently a non-empty check).
        if let Err(e) = common::ImageUrl::new(self.game_host_image.clone()) {
            return Err(ConfigError::invalid(
                env_names::GAME_HOST_IMAGE,
                self.game_host_image.clone(),
                e.to_string(),
            ));
        }

        let docker_network = match self.machine_provider {
            MachineProviderKind::Docker => {
                let net = self.docker_network.clone().unwrap_or_default();
                if net.trim().is_empty() {
                    return Err(ConfigError::missing(
                        env_names::DOCKER_NETWORK,
                        "required when MACHINE_PROVIDER=docker (the shared network the website is on)",
                    ));
                }
                Some(net)
            }
            MachineProviderKind::Microsandbox => self.docker_network.clone(),
        };

        // Reaper prefix defaults to the agent name prefix (single source so
        // naming and reaping cannot drift).
        let agent_name_prefix = self.agent_name_prefix.clone();
        let reaper_prefix = self
            .reaper_prefix
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| agent_name_prefix.clone());

        Ok(Some(ResolvedCoordinator {
            provider: self.machine_provider,
            game_host_image: self.game_host_image.clone(),
            agents_per_game: self.agents_per_game,
            tick_rate_ms: self.game_tick_rate_ms,
            game_interval_secs: self.game_interval_secs,
            game_host_connect_timeout_secs: self.game_host_connect_timeout_secs,
            game_host_grpc_port: env_names::DEFAULT_GAME_HOST_GRPC_PORT,
            agent_grpc_port: env_names::DEFAULT_AGENT_GRPC_PORT,
            docker_network,
            registry_pull_host: self.docker_registry_pull_host.clone(),
            agent_name_prefix,
            machine_cpus: self.machine_cpus,
            machine_memory_mib: self.machine_mem_mib,
            msb_host_port_base: self.msb_host_port_base,
            msb_host_bind: self.msb_host_bind,
            msb_registry_insecure: self.msb_registry_insecure,
            msb_max_duration_secs: self.msb_max_duration_secs,
            reaper_interval_secs: self.reaper_interval_secs,
            reaper_max_age_secs: self.reaper_max_age_secs,
            reaper_prefix,
        }))
    }
}

fn non_empty_opt(v: Option<String>, key: &'static str, hint: &str) -> Result<String, ConfigError> {
    match v {
        Some(s) if !s.trim().is_empty() => Ok(s),
        _ => Err(ConfigError::missing(key, hint)),
    }
}

/// Translate a `config::ConfigError` into the most helpful `ConfigError`.
///
/// `Environment` lowercases keys, so the message names the snake_case field
/// (``for key `port` ``) or the missing field (`"database_url"`); uppercasing
/// recovers the env var name with no per-field table.
fn map_serde_error(e: config::ConfigError, map: &HashMap<String, String>) -> ConfigError {
    let msg = e.to_string();
    // Prefer the backtick-quoted key (type errors); fall back to the
    // double-quoted field (missing-field errors, which carry no backticks).
    let key = msg
        .rsplit('`')
        .nth(1)
        .or_else(|| msg.rsplit('"').nth(1))
        .unwrap_or_default();
    let found = ALL_ENV_VARS
        .iter()
        .copied()
        .find(|var| var.eq_ignore_ascii_case(key));
    match found {
        Some(var) => {
            let value = map.get(var).cloned().unwrap_or_default();
            if msg.starts_with("missing configuration field") {
                ConfigError::missing(var, missing_hint(var))
            } else {
                let mut reason = msg;
                if var == env_names::MACHINE_PROVIDER {
                    reason += " (expected \"docker\" or \"microsandbox\")";
                }
                ConfigError::invalid(var, value, reason)
            }
        }
        None => ConfigError::Config(e),
    }
}

/// Human hints for required vars; everything else points at `.env.example`.
fn missing_hint(var: &str) -> String {
    match var {
        env_names::GITHUB_CLIENT_ID => "GitHub OAuth app client id".to_string(),
        env_names::GITHUB_CLIENT_SECRET => "GitHub OAuth app client secret".to_string(),
        env_names::DATABASE_URL => {
            "Postgres connection string, e.g. postgresql://arcadio:arcadio@localhost:5432/arcadio"
                .to_string()
        }
        env_names::REGISTRY_PRIVATE_KEY => {
            "RSA private key (PEM) used to mint registry JWTs".to_string()
        }
        _ => "required; see .env.example".to_string(),
    }
}
