//! Single human-readable error type for all startup config parsing.

/// Fail-fast error returned by every `*_Config::from_env` / `from_map`.
///
/// `main` prints exactly one of these via its `Display` impl instead of
/// panicking deep in `serve`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required variable is absent (or empty where empty is not allowed).
    #[error("missing required env var {key}: {hint}")]
    Missing { key: &'static str, hint: String },

    /// A variable is present but unparsable or semantically invalid.
    #[error("invalid {key}={value:?}: {reason}")]
    Invalid {
        key: &'static str,
        value: String,
        reason: String,
    },

    /// Cross-field violation (e.g. docker backend without its network).
    #[error("{message} (env var {key})")]
    Conflict { key: &'static str, message: String },

    /// Wraps `config::ConfigError` (bad defaults, type coercion, …).
    /// Should be rare: known vars get precise `Missing`/`Invalid` above.
    #[error("configuration error: {0}")]
    Config(#[from] config::ConfigError),
}

impl ConfigError {
    pub fn missing(key: &'static str, hint: impl Into<String>) -> Self {
        Self::Missing {
            key,
            hint: hint.into(),
        }
    }

    pub fn invalid(key: &'static str, value: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Invalid {
            key,
            value: value.into(),
            reason: reason.into(),
        }
    }

    pub fn conflict(key: &'static str, message: impl Into<String>) -> Self {
        Self::Conflict {
            key,
            message: message.into(),
        }
    }
}
