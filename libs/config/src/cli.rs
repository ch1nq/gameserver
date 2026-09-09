//! Typed CLI configuration, sharing `ACHTUNG_*` name constants.
//!
//! Loading is declarative: an optional TOML file layer under an environment
//! layer (env wins), deserialized once into [`RawCli`]. Precedence is source
//! ordering — defaults < file < env — not hand-written merge code. Field
//! `rename`s name the full env var, so errors and `.env.example` name the
//! same thing; `alias`es accept the bare `config.toml` keys.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::env_names;
use crate::error::ConfigError;
use crate::support::{serde_error_key, setting};

setting!(
    DEFAULT_REGISTRY_HOST,
    default_registry_host,
    &str,
    "localhost:5001"
);

/// Validated CLI runtime config (all fields resolved).
#[derive(Debug, Clone)]
pub struct CliConfig {
    pub api_url: String,
    pub user_id: i64,
    pub api_token: String,
    pub registry_host: String,
}

/// Flat deserialization target shared by the file and env layers.
///
/// Both layers speak bare `snake_case` keys — the TOML file natively, and env
/// via `with_prefix("ACHTUNG")` stripping (`ACHTUNG_API_URL` → `api_url`) — so
/// the crate merges them natively with env winning, and no field is ever seen
/// twice under two spellings.
#[derive(Debug, Deserialize)]
struct RawCli {
    api_url: Option<String>,
    user_id: Option<i64>,
    api_token: Option<String>,
    #[serde(default = "default_registry_host")]
    registry_host: String,
}

impl CliConfig {
    /// Load from process env + optional toml file (env wins).
    pub fn from_env_or_file() -> Result<Self, ConfigError> {
        let file = load_file().map_err(|e| {
            ConfigError::invalid(
                env_names::ACHTUNG_API_URL,
                String::new(),
                format!("failed to read CLI config file: {e}"),
            )
        })?;
        Self::load(file.as_ref(), None)
    }

    /// Injectable variant for tests (no fs/env access).
    pub fn from_map_with_file(
        map: HashMap<String, String>,
        file: Option<CliFileParsed>,
    ) -> Result<Self, ConfigError> {
        Self::load(file.as_ref(), Some(map))
    }

    /// Simple map-only parse (no file), used by unit tests.
    pub fn from_map(map: HashMap<String, String>) -> Result<Self, ConfigError> {
        Self::load(None, Some(map))
    }

    /// Defaults < TOML file < env, deserialized once into [`RawCli`].
    fn load(
        file: Option<&CliFileParsed>,
        env: Option<HashMap<String, String>>,
    ) -> Result<Self, ConfigError> {
        let snapshot = env.clone().unwrap_or_default();
        let mut builder = config::Config::builder();
        if let Some(f) = file {
            // Reuse the `File` source so precedence stays declarative: the
            // parsed file content is re-encoded (empty doc when all `None`).
            let toml_str = toml::to_string(f).map_err(|e| {
                ConfigError::invalid(
                    env_names::ACHTUNG_API_URL,
                    String::new(),
                    format!("failed to encode CLI file config: {e}"),
                )
            })?;
            builder =
                builder.add_source(config::File::from_str(&toml_str, config::FileFormat::Toml));
        }
        builder = builder.add_source(
            config::Environment::with_prefix("ACHTUNG")
                .try_parsing(true)
                .ignore_empty(true)
                .source(env),
        );
        let raw: RawCli = builder
            .build()
            .map_err(ConfigError::from)?
            .try_deserialize()
            .map_err(|e| map_serde_error(e, &snapshot))?;
        raw.resolve()
    }
}

impl RawCli {
    /// Required-field checks with hints pointing at both configuration sites.
    /// Everything the derive layer cannot express.
    fn resolve(self) -> Result<CliConfig, ConfigError> {
        let path = config_path();
        let api_url = match self.api_url.filter(|s| !s.trim().is_empty()) {
            Some(s) => s,
            None => {
                return Err(ConfigError::missing(
                    env_names::ACHTUNG_API_URL,
                    format!(
                        "set {} or add api_url to {}",
                        env_names::ACHTUNG_API_URL,
                        path.display()
                    ),
                ));
            }
        };
        let user_id = match self.user_id {
            Some(id) => id,
            None => {
                return Err(ConfigError::missing(
                    env_names::ACHTUNG_USER_ID,
                    format!(
                        "set {} or add user_id to {}",
                        env_names::ACHTUNG_USER_ID,
                        path.display()
                    ),
                ));
            }
        };
        let api_token = match self.api_token.filter(|s| !s.trim().is_empty()) {
            Some(s) => s,
            None => {
                return Err(ConfigError::missing(
                    env_names::ACHTUNG_API_TOKEN,
                    format!(
                        "set {} or add api_token to {}",
                        env_names::ACHTUNG_API_TOKEN,
                        path.display()
                    ),
                ));
            }
        };
        Ok(CliConfig {
            api_url,
            user_id,
            api_token,
            registry_host: self.registry_host,
        })
    }
}

/// Translate a load error back to the `ACHTUNG_*` name: messages carry the
/// bare key (``for key `user_id` `` / `"user_id"`), and the four fields map
/// 1:1 onto their env vars.
fn map_serde_error(e: config::ConfigError, map: &HashMap<String, String>) -> ConfigError {
    let msg = e.to_string();
    let found = match serde_error_key(&msg) {
        "api_url" => Some(env_names::ACHTUNG_API_URL),
        "user_id" => Some(env_names::ACHTUNG_USER_ID),
        "api_token" => Some(env_names::ACHTUNG_API_TOKEN),
        "registry_host" => Some(env_names::ACHTUNG_REGISTRY_HOST),
        _ => None,
    };
    match found {
        Some(var) => {
            let value = map.get(var).cloned().unwrap_or_default();
            if msg.starts_with("missing configuration field") {
                ConfigError::missing(
                    var,
                    format!("set {var} or add it to {}", config_path().display()),
                )
            } else {
                ConfigError::invalid(var, value, msg)
            }
        }
        None => ConfigError::Config(e),
    }
}

/// On-disk `config.toml` shape (all fields optional); doubles as the file
/// layer input, so the two can never drift apart.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CliFileParsed {
    pub api_url: Option<String>,
    pub user_id: Option<i64>,
    pub api_token: Option<String>,
    pub registry_host: Option<String>,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("achtung")
        .join("config.toml")
}

fn load_file() -> Result<Option<CliFileParsed>, String> {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let f: CliFileParsed = toml::from_str(&contents)
                .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
            Ok(Some(f))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("failed to read {}: {e}", path.display())),
    }
}
