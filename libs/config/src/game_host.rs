//! Typed game-host configuration (`achtung-host` binary).
//!
//! Unifies the arena defaults: both dimensions default to 1000² (previously
//! `AchtungConfig::default()` was 1000x200 while `from_env()` defaulted to
//! 1000x1000). `PORT` shares its *name* with the website via `env_names::PORT`
//! but keeps its own default (`50051`).

use std::collections::HashMap;

use serde::Deserialize;

use crate::env_names;
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

#[derive(Debug, Deserialize)]
struct RawGameHost {
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_width")]
    arena_width: u32,
    #[serde(default = "default_height")]
    arena_height: u32,
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
        let map: HashMap<String, String> = std::env::vars().collect();
        Self::from_map(map)
    }

    pub fn from_map(map: HashMap<String, String>) -> Result<Self, ConfigError> {
        let mut b = config::Config::builder();
        b = b
            .set_default("port", i64::from(DEFAULT_GAME_HOST_PORT))
            .map_err(ConfigError::from)?;
        b = b
            .set_default("arena_width", i64::from(DEFAULT_ARENA_WIDTH))
            .map_err(ConfigError::from)?;
        b = b
            .set_default("arena_height", i64::from(DEFAULT_ARENA_HEIGHT))
            .map_err(ConfigError::from)?;
        b = b
            .set_default("rust_log", default_log())
            .map_err(ConfigError::from)?;

        for (nested, flat) in [
            ("port", env_names::PORT),
            ("arena_width", env_names::ARENA_WIDTH),
            ("arena_height", env_names::ARENA_HEIGHT),
            ("rust_log", env_names::RUST_LOG),
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

        let built = b.build().map_err(ConfigError::from)?;
        let raw: RawGameHost = built.try_deserialize().map_err(|e| {
            let msg = e.to_string();
            if msg.contains("arena_width") {
                ConfigError::invalid(
                    env_names::ARENA_WIDTH,
                    map.get(env_names::ARENA_WIDTH).cloned().unwrap_or_default(),
                    msg,
                )
            } else if msg.contains("arena_height") {
                ConfigError::invalid(
                    env_names::ARENA_HEIGHT,
                    map.get(env_names::ARENA_HEIGHT)
                        .cloned()
                        .unwrap_or_default(),
                    msg,
                )
            } else if msg.contains("port") {
                ConfigError::invalid(
                    env_names::PORT,
                    map.get(env_names::PORT).cloned().unwrap_or_default(),
                    msg,
                )
            } else {
                ConfigError::Config(e)
            }
        })?;

        if raw.port == 0 {
            return Err(ConfigError::invalid(
                env_names::PORT,
                "0",
                "must be a non-zero port",
            ));
        }
        if raw.arena_width == 0 {
            return Err(ConfigError::invalid(
                env_names::ARENA_WIDTH,
                "0",
                "must be at least 1",
            ));
        }
        if raw.arena_height == 0 {
            return Err(ConfigError::invalid(
                env_names::ARENA_HEIGHT,
                "0",
                "must be at least 1",
            ));
        }

        Ok(Self {
            port: raw.port,
            arena_width: raw.arena_width,
            arena_height: raw.arena_height,
            rust_log: raw.rust_log,
        })
    }
}
