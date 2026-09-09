//! Typed game-host configuration (`achtung-host` binary).
//!
//! Unifies the arena defaults: both dimensions default to 1000² (previously
//! `AchtungConfig::default()` was 1000x200 while `from_env()` defaulted to
//! 1000x1000). `PORT` shares its *name* with the website via `env_names::PORT`
//! but keeps its own default (`50051`). Loading mirrors [`crate::website`]:
//! one [`config::Environment`] source over a flat raw struct, semantic checks
//! in [`RawGameHost::resolve`].

use std::collections::HashMap;

use serde::Deserialize;

use crate::env_names::{self, ALL_ENV_VARS};
use crate::error::ConfigError;

pub const DEFAULT_GAME_HOST_PORT: u16 = 50051;
pub const DEFAULT_ARENA_WIDTH: u32 = 1000;
pub const DEFAULT_ARENA_HEIGHT: u32 = 1000;

/// Validated game-host startup config.
#[derive(Debug, Clone)]
pub struct GameHostConfig {
    /// Listen port. Env: `PORT` (default `50051`).
    pub port: u16,
    /// Arena width. Env: `ARENA_WIDTH` (default `1000`, must be > 0).
    pub arena_width: u32,
    /// Arena height. Env: `ARENA_HEIGHT` (default `1000`, must be > 0).
    pub arena_height: u32,
    /// Tracing filter. Env: `RUST_LOG` (default `achtung_host=info,arcadio=info,info`).
    pub rust_log: String,
}

/// Flat deserialization target; snake_case fields bind their `SCREAMING` vars.
#[derive(Debug, Deserialize)]
struct RawGameHost {
    /// Env: `PORT` (default `50051`).
    #[serde(default = "default_port")]
    port: u16,
    /// Env: `ARENA_WIDTH` (default `1000`, must be > 0).
    #[serde(default = "default_width")]
    arena_width: u32,
    /// Env: `ARENA_HEIGHT` (default `1000`, must be > 0).
    #[serde(default = "default_height")]
    arena_height: u32,
    /// Env: `RUST_LOG` (default `achtung_host=info,arcadio=info,info`).
    #[serde(default = "default_log")]
    rust_log: String,
}

fn default_port() -> u16 {
    DEFAULT_GAME_HOST_PORT
}
fn default_width() -> u32 {
    DEFAULT_ARENA_WIDTH
}
fn default_height() -> u32 {
    DEFAULT_ARENA_HEIGHT
}
fn default_log() -> String {
    "achtung_host=info,arcadio=info,info".to_string()
}

impl GameHostConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_map(std::env::vars().collect())
    }

    pub fn from_map(map: HashMap<String, String>) -> Result<Self, ConfigError> {
        let snapshot = map.clone();
        let raw: RawGameHost = config::Config::builder()
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

impl RawGameHost {
    fn resolve(self) -> Result<GameHostConfig, ConfigError> {
        if self.port == 0 {
            return Err(ConfigError::invalid(
                env_names::PORT,
                "0",
                "must be a non-zero port",
            ));
        }
        if self.arena_width == 0 {
            return Err(ConfigError::invalid(
                env_names::ARENA_WIDTH,
                "0",
                "must be at least 1",
            ));
        }
        if self.arena_height == 0 {
            return Err(ConfigError::invalid(
                env_names::ARENA_HEIGHT,
                "0",
                "must be at least 1",
            ));
        }

        Ok(GameHostConfig {
            port: self.port,
            arena_width: self.arena_width,
            arena_height: self.arena_height,
            rust_log: self.rust_log,
        })
    }
}

/// Recover the env var name from the message (see [`crate::website`]) — here
/// the only possible keys are the four fields below.
fn map_serde_error(e: config::ConfigError, map: &HashMap<String, String>) -> ConfigError {
    let msg = e.to_string();
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
            ConfigError::invalid(var, value, msg)
        }
        None => ConfigError::Config(e),
    }
}
