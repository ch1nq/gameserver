//! Typed CLI configuration, sharing `ACHTUNG_*` name constants.
//!
//! Preserves the existing precedence: explicit env vars win, then
//! `~/.config/achtung/config.toml` (or `$XDG_CONFIG_HOME`), then built-in
//! defaults. Fail-fast with [`ConfigError`] instead of ad-hoc strings.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::env_names;
use crate::error::ConfigError;

pub const DEFAULT_REGISTRY_HOST: &str = "localhost:5001";

/// Raw on-disk format (all fields optional).
#[derive(Debug, Default, Deserialize)]
struct CliFile {
    api_url: Option<String>,
    user_id: Option<i64>,
    api_token: Option<String>,
    registry_host: Option<String>,
}

/// Validated CLI runtime config (all fields resolved).
#[derive(Debug, Clone)]
pub struct CliConfig {
    pub api_url: String,
    pub user_id: i64,
    pub api_token: String,
    pub registry_host: String,
}

impl CliConfig {
    /// Load from process env + optional toml file (env wins).
    pub fn from_env_or_file() -> Result<Self, ConfigError> {
        let map: HashMap<String, String> = std::env::vars().collect();
        let file = load_file().map_err(|e| {
            ConfigError::invalid(
                env_names::ACHTUNG_API_URL,
                String::new(),
                format!("failed to read CLI config file: {e}"),
            )
        })?;
        Self::from_map_with_file(map, file)
    }

    /// Injectable variant for tests (no fs/env access).
    pub fn from_map_with_file(
        map: HashMap<String, String>,
        file: Option<CliFileParsed>,
    ) -> Result<Self, ConfigError> {
        // Build a `config` crate layer: file values as defaults, env as overrides.
        let mut b = config::Config::builder();
        if let Some(f) = &file {
            if let Some(v) = &f.api_url {
                b = b
                    .set_default("api_url", v.clone())
                    .map_err(ConfigError::from)?;
            }
            if let Some(v) = f.user_id {
                b = b.set_default("user_id", v).map_err(ConfigError::from)?;
            }
            if let Some(v) = &f.api_token {
                b = b
                    .set_default("api_token", v.clone())
                    .map_err(ConfigError::from)?;
            }
            if let Some(v) = &f.registry_host {
                b = b
                    .set_default("registry_host", v.clone())
                    .map_err(ConfigError::from)?;
            }
        }
        b = b
            .set_default("registry_host", DEFAULT_REGISTRY_HOST)
            .map_err(ConfigError::from)?;

        for (nested, flat) in [
            ("api_url", env_names::ACHTUNG_API_URL),
            ("user_id", env_names::ACHTUNG_USER_ID),
            ("api_token", env_names::ACHTUNG_API_TOKEN),
            ("registry_host", env_names::ACHTUNG_REGISTRY_HOST),
        ] {
            if let Some(v) = map.get(flat) {
                if v.is_empty() {
                    continue;
                }
                b = b
                    .set_override(nested, v.clone())
                    .map_err(|e| ConfigError::invalid(flat, v.clone(), e.to_string()))?;
            }
        }

        #[derive(Debug, Deserialize)]
        struct Raw {
            api_url: Option<String>,
            user_id: Option<i64>,
            api_token: Option<String>,
            #[serde(default = "default_registry_host")]
            registry_host: String,
        }
        fn default_registry_host() -> String {
            DEFAULT_REGISTRY_HOST.to_string()
        }

        let built = b.build().map_err(ConfigError::from)?;
        let raw: Raw = built.try_deserialize().map_err(|e| {
            let msg = e.to_string();
            if msg.contains("user_id") {
                ConfigError::invalid(
                    env_names::ACHTUNG_USER_ID,
                    map.get(env_names::ACHTUNG_USER_ID)
                        .cloned()
                        .unwrap_or_default(),
                    format!("must be an integer user id: {msg}"),
                )
            } else {
                ConfigError::Config(e)
            }
        })?;

        let api_url = match raw.api_url.filter(|s| !s.trim().is_empty()) {
            Some(s) => s,
            None => {
                return Err(ConfigError::missing(
                    env_names::ACHTUNG_API_URL,
                    format!(
                        "set {} or add api_url to {}",
                        env_names::ACHTUNG_API_URL,
                        config_path().display()
                    ),
                ));
            }
        };
        let user_id = match raw.user_id {
            Some(id) => id,
            None => {
                return Err(ConfigError::missing(
                    env_names::ACHTUNG_USER_ID,
                    format!(
                        "set {} or add user_id to {}",
                        env_names::ACHTUNG_USER_ID,
                        config_path().display()
                    ),
                ));
            }
        };
        let api_token = match raw.api_token.filter(|s| !s.trim().is_empty()) {
            Some(s) => s,
            None => {
                return Err(ConfigError::missing(
                    env_names::ACHTUNG_API_TOKEN,
                    format!(
                        "set {} or add api_token to {}",
                        env_names::ACHTUNG_API_TOKEN,
                        config_path().display()
                    ),
                ));
            }
        };
        Ok(Self {
            api_url,
            user_id,
            api_token,
            registry_host: raw.registry_host,
        })
    }

    /// Simple map-only parse (no file), used by unit tests.
    pub fn from_map(map: HashMap<String, String>) -> Result<Self, ConfigError> {
        Self::from_map_with_file(map, None)
    }
}

/// Parsed file contents, exposed for `from_map_with_file` tests.
#[derive(Debug, Clone)]
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
            let f: CliFile = toml::from_str(&contents)
                .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
            Ok(Some(CliFileParsed {
                api_url: f.api_url,
                user_id: f.user_id,
                api_token: f.api_token,
                registry_host: f.registry_host,
            }))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("failed to read {}: {e}", path.display())),
    }
}
