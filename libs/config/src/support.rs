//! Shared loading plumbing: default definitions and serde-error mapping.
//!
//! The per-area modules (`website`, `game_host`, `cli`) own their raw structs
//! and semantic checks; everything here is mechanical and identical across
//! areas, so it lives once.

use std::collections::HashMap;

use crate::env_names::{self, ALL_ENV_VARS};
use crate::error::ConfigError;

/// Defines a documented default once: a `pub const` holding the value plus
/// the zero-arg `fn` that `#[serde(default = "...")]` attributes reference.
///
/// Without this, every setting pays for both spellings by hand (~3 lines
/// each); with it, one line per setting.
macro_rules! setting {
    ($const:ident, $func:ident, &str, $val:expr) => {
        pub const $const: &str = $val;
        fn $func() -> String {
            $val.to_owned()
        }
    };
    ($const:ident, $func:ident, $ty:ty, $val:expr) => {
        pub const $const: $ty = $val;
        fn $func() -> $ty {
            $val
        }
    };
}

pub(crate) use setting;

/// `Some` non-blank string or a `Missing` error with a human hint.
pub(crate) fn non_empty_opt(
    v: Option<String>,
    key: &'static str,
    hint: &str,
) -> Result<String, ConfigError> {
    match v {
        Some(s) if !s.trim().is_empty() => Ok(s),
        _ => Err(ConfigError::missing(key, hint)),
    }
}

/// Recover the offending field from a `config` error message.
///
/// Type errors quote the key in backticks (``for key `port` ``); missing-field
/// errors carry no backticks, so fall back to the double-quoted field
/// (`missing configuration field "database_url"`).
pub(crate) fn serde_error_key(msg: &str) -> &str {
    msg.rsplit('`')
        .nth(1)
        .or_else(|| msg.rsplit('"').nth(1))
        .unwrap_or_default()
}

/// Translate a `config::ConfigError` into the most helpful `ConfigError`.
///
/// `Environment` lowercases keys, so the message names the snake_case field;
/// case-insensitive comparison recovers the `SCREAMING` env var with no
/// per-field table.
pub(crate) fn map_serde_error(
    e: config::ConfigError,
    map: &HashMap<String, String>,
) -> ConfigError {
    let msg = e.to_string();
    let found = ALL_ENV_VARS
        .iter()
        .copied()
        .find(|var| var.eq_ignore_ascii_case(serde_error_key(&msg)));
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
pub(crate) fn missing_hint(var: &str) -> String {
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
