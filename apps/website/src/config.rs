//! One typed configuration for the website, parsed once at startup with
//! fail-fast validation.
//!
//! Settings are layered by [`figment`]: a `config.toml` file (see
//! `config.example.toml`) provides the base, and environment variables override
//! it — so a deployment can commit non-secret defaults in the file and inject
//! secrets via the environment.
//!
//! The shape is nested, and figment derives it directly: TOML uses tables
//! (`[coordinator.docker]`), and the environment uses `__` between levels
//! (`COORDINATOR__DOCKER__NETWORK`). Because the nesting supplies the domain
//! prefix, the struct fields carry only the bare name (`network`), which lets
//! the coordinator/agent-infra config structs deserialize as-is instead of
//! through a hand-written mapping layer.
//!
//! A missing required setting or an invalid value fails *before* any
//! `TcpListener`/DB work — figment reports missing/mistyped fields, and
//! [`Config::load`] adds the few semantic checks serde cannot express (a valid
//! `game_host_image`, and `docker.network` present when the docker backend is
//! selected).

use std::net::SocketAddr;
use std::time::Duration;

use agent_infra::{DockerMachineProviderConfig, MicrosandboxMachineProviderConfig};
use coordinator::ImageUrl;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;

/// Config file path, overridable with `CONFIG_FILE`. A missing file is not an
/// error — the environment alone can supply every setting.
const DEFAULT_CONFIG_FILE: &str = "config.toml";

/// Ports the coordinator dials *inside* each machine. Not configurable: the
/// game host and agent images bake these in, so exposing them as knobs would
/// only invite drift between image and coordinator.
const GAME_HOST_GRPC_PORT: u16 = 50051;
const AGENT_GRPC_PORT: u16 = 50052;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid configuration: {0}")]
    Extract(#[from] figment::Error),

    #[error("coordinator.game_host_image is not a valid image URL: {0}")]
    InvalidGameHostImage(String),

    #[error(
        "coordinator.docker.network is required when the coordinator is enabled with provider = \"docker\""
    )]
    MissingDockerNetwork,

    #[error("server host/port do not form a valid socket address ({host}:{port}): {source}")]
    InvalidSocketAddr {
        host: String,
        port: u16,
        source: std::net::AddrParseError,
    },
}

/// Fully parsed website configuration. figment builds this directly from the
/// file + environment; [`Config::load`] then runs [`Config::validate`].
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    pub github: GithubConfig,
    pub database_url: String,
    #[serde(default)]
    pub registry: RegistryConfig,
    #[serde(default)]
    pub coordinator: CoordinatorSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryConfig {
    /// RSA private key (PEM) used to mint registry auth tokens. Required.
    pub private_key: String,
    /// JWT audience; must equal the registry's `REGISTRY_AUTH_TOKEN_SERVICE`.
    #[serde(default = "default_registry_service")]
    pub service: String,
    /// How this server reaches the registry API.
    #[serde(default = "default_registry_url")]
    pub url: String,
    /// Host rendered into user-facing `docker login/tag/push` hints; must be
    /// reachable from the user's machine, never the in-network name.
    #[serde(default = "default_registry_public_host")]
    pub public_host: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            private_key: String::new(),
            service: default_registry_service(),
            url: default_registry_url(),
            public_host: default_registry_public_host(),
        }
    }
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

/// Machine backend selector. Deriving `Deserialize` lets figment reject an
/// unknown `provider` for us, with its own "unknown variant" error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MachineProviderKind {
    Docker,
    Microsandbox,
}

/// Coordinator + reaper settings. When `enabled` is false the server runs
/// without spawning matches (the spectator endpoint still exists, just idle),
/// and the rest of these fields are ignored.
///
/// The `docker`/`microsandbox` sub-configs are the real `agent-infra` types,
/// deserialized in place; `provider` selects which one is used.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CoordinatorSettings {
    pub enabled: bool,
    pub provider: MachineProviderKind,
    pub game_host_image: String,
    pub agents_per_game: usize,
    pub tick_rate_ms: u64,
    pub game_interval_secs: u64,
    pub connect_timeout_secs: u64,
    pub arena_width: u32,
    pub arena_height: u32,
    /// Present only for the docker backend; `network` has no default, so a
    /// docker deployment must supply it.
    pub docker: Option<DockerMachineProviderConfig>,
    pub microsandbox: MicrosandboxMachineProviderConfig,
    pub reaper: ReaperSettings,
}

impl Default for CoordinatorSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: MachineProviderKind::Microsandbox,
            game_host_image: "ghcr.io/ch1nq/achtung-game-host:latest".to_string(),
            agents_per_game: 4,
            tick_rate_ms: 50,
            game_interval_secs: 10,
            connect_timeout_secs: 60,
            arena_width: 1000,
            arena_height: 1000,
            docker: None,
            microsandbox: MicrosandboxMachineProviderConfig::default(),
            reaper: ReaperSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ReaperSettings {
    pub interval_secs: u64,
    pub max_age_secs: u64,
    /// Substring used to match this backend's resources for orphan cleanup.
    pub prefix: String,
}

impl Default for ReaperSettings {
    fn default() -> Self {
        Self {
            interval_secs: 300,
            max_age_secs: 3600,
            prefix: "achtung-".to_string(),
        }
    }
}

impl Config {
    /// Parse and validate configuration, layering `config.toml` (base) under
    /// environment-variable overrides (`__` separates nesting levels).
    pub fn load() -> Result<Self, ConfigError> {
        let path = std::env::var("CONFIG_FILE").unwrap_or_else(|_| DEFAULT_CONFIG_FILE.to_string());
        let config: Config = Figment::new()
            .merge(Toml::file(path))
            .merge(Env::raw().split("__"))
            .extract()?;
        config.validate()?;
        Ok(config)
    }

    /// Semantic checks serde cannot express. Only the enabled coordinator is
    /// checked; a disabled one may hold defaults that are never used.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.coordinator.enabled {
            // Validate the image URL up front so a typo fails here, not on the
            // first spawn.
            self.coordinator.game_host_image_url()?;
            if self.coordinator.provider == MachineProviderKind::Docker
                && self.coordinator.docker.is_none()
            {
                return Err(ConfigError::MissingDockerNetwork);
            }
        }
        Ok(())
    }

    /// Resolve the `host:port` bind address, failing fast on a malformed host.
    pub fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
        format!("{}:{}", self.server.host, self.server.port)
            .parse()
            .map_err(|source| ConfigError::InvalidSocketAddr {
                host: self.server.host.clone(),
                port: self.server.port,
                source,
            })
    }
}

impl CoordinatorSettings {
    /// The validated game-host image URL.
    pub fn game_host_image_url(&self) -> Result<ImageUrl, ConfigError> {
        ImageUrl::new(self.game_host_image.clone())
            .map_err(|e| ConfigError::InvalidGameHostImage(e.to_string()))
    }

    /// Build the coordinator's own config. `poll_interval` and the in-machine
    /// gRPC ports are not configurable, so they are filled in here. Panics only
    /// if called on an unvalidated config with a bad image (`load` validates).
    pub fn coordinator_config(&self) -> coordinator::CoordinatorConfig {
        coordinator::CoordinatorConfig {
            game_host_image: self
                .game_host_image_url()
                .expect("game_host_image validated in Config::load"),
            agents_per_game: self.agents_per_game,
            tick_rate_ms: self.tick_rate_ms,
            arena_width: self.arena_width,
            arena_height: self.arena_height,
            game_interval: Duration::from_secs(self.game_interval_secs),
            poll_interval: Duration::from_secs(1),
            game_host_grpc_port: GAME_HOST_GRPC_PORT,
            agent_grpc_port: AGENT_GRPC_PORT,
            game_host_connect_timeout: Duration::from_secs(self.connect_timeout_secs),
        }
    }

    /// Build the reaper's runtime config (seconds -> `Duration`).
    pub fn reaper_config(&self) -> agent_infra::ReaperConfig {
        agent_infra::ReaperConfig {
            interval: Duration::from_secs(self.reaper.interval_secs),
            max_age: Duration::from_secs(self.reaper.max_age_secs),
            prefix: self.reaper.prefix.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set the four required keys so `Config::load` gets past figment's
    /// missing-field checks; individual tests add whatever else they exercise.
    fn set_required(jail: &mut figment::Jail) {
        jail.set_env("GITHUB__CLIENT_ID", "id");
        jail.set_env("GITHUB__CLIENT_SECRET", "secret");
        jail.set_env("DATABASE_URL", "postgres://localhost/db");
        jail.set_env("REGISTRY__PRIVATE_KEY", "pem");
    }

    #[test]
    fn coordinator_disabled_by_default() {
        figment::Jail::expect_with(|jail| {
            set_required(jail);
            assert!(!Config::load().unwrap().coordinator.enabled);
            Ok(())
        });
    }

    #[test]
    fn env_overrides_file_and_missing_file_is_fine() {
        figment::Jail::expect_with(|jail| {
            // No config.toml yet: the environment alone must suffice.
            set_required(jail);
            assert_eq!(Config::load().unwrap().github.client_id, "id");

            // A file provides a base value that the environment overrides.
            jail.create_file(
                "config.toml",
                r#"
                [github]
                client_id = "from-file"
                [registry]
                url = "http://from-file:5001"
                "#,
            )?;
            let config = Config::load().unwrap();
            assert_eq!(config.github.client_id, "id", "env should win over file");
            assert_eq!(config.registry.url, "http://from-file:5001");
            Ok(())
        });
    }

    #[test]
    fn unknown_machine_provider_fails_fast() {
        // Rejected by figment/serde at deserialize time, not by hand-written code.
        figment::Jail::expect_with(|jail| {
            set_required(jail);
            jail.set_env("COORDINATOR__PROVIDER", "podman");
            assert!(matches!(Config::load(), Err(ConfigError::Extract(_))));
            Ok(())
        });
    }

    #[test]
    fn docker_requires_a_network() {
        figment::Jail::expect_with(|jail| {
            set_required(jail);
            jail.set_env("COORDINATOR__ENABLED", "true");
            jail.set_env("COORDINATOR__PROVIDER", "docker");
            assert!(matches!(
                Config::load(),
                Err(ConfigError::MissingDockerNetwork)
            ));
            Ok(())
        });
    }

    #[test]
    fn docker_backend_is_built_from_nested_keys() {
        figment::Jail::expect_with(|jail| {
            set_required(jail);
            jail.set_env("COORDINATOR__ENABLED", "true");
            jail.set_env("COORDINATOR__PROVIDER", "docker");
            jail.set_env("COORDINATOR__DOCKER__NETWORK", "gameserver_default");

            let coordinator = Config::load().unwrap().coordinator;
            let docker = coordinator.docker.expect("docker config present");
            assert_eq!(docker.network, "gameserver_default");
            // Field defaults on the agent-infra struct still apply.
            assert_eq!(docker.name_prefix, "achtung-");
            Ok(())
        });
    }

    #[test]
    fn microsandbox_is_the_default_backend() {
        figment::Jail::expect_with(|jail| {
            set_required(jail);
            jail.set_env("COORDINATOR__ENABLED", "true");
            jail.set_env("COORDINATOR__MICROSANDBOX__REGISTRY_INSECURE", "true");

            let coordinator = Config::load().unwrap().coordinator;
            assert_eq!(coordinator.provider, MachineProviderKind::Microsandbox);
            assert_eq!(coordinator.arena_width, 1000);
            assert!(coordinator.microsandbox.registry_insecure);
            assert_eq!(coordinator.microsandbox.host_port_base, 51000);
            Ok(())
        });
    }

    #[test]
    fn example_config_file_parses() {
        // Guards against drift between config.example.toml and the schema.
        let example = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config.example.toml");
        figment::Jail::expect_with(|jail| {
            jail.set_env("CONFIG_FILE", example);
            let config = Config::load().expect("config.example.toml should parse and validate");
            assert!(config.coordinator.enabled);
            Ok(())
        });
    }

    #[test]
    fn invalid_game_host_image_fails_fast() {
        figment::Jail::expect_with(|jail| {
            set_required(jail);
            jail.set_env("COORDINATOR__ENABLED", "true");
            jail.set_env("COORDINATOR__GAME_HOST_IMAGE", "   ");
            assert!(matches!(
                Config::load(),
                Err(ConfigError::InvalidGameHostImage(_))
            ));
            Ok(())
        });
    }
}
