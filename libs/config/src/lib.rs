//! Centralized typed startup configuration (fixes #27, closes #11).
//!
//! One `*_Config::from_env()` per binary, parsed once at startup with
//! fail-fast [`ConfigError`]. All env var *names* live in [`env_names`] so the
//! website, coordinator, game host, and CLI cannot drift apart.
//!
//! Built on the [`config`] crate + [`serde`]: declarative `File`/`Environment`
//! sources layer defaults < file < env over flat raw structs (field attributes
//! carry defaults and bindings), and `try_deserialize` coerces `"50"` → `50`,
//! `"true"` → `true`, etc. Semantic validation lives in small `resolve`
//! methods. Tests use `from_map` with an explicit map so process env is never
//! mutated.

pub mod cli;
pub mod env_names;
pub mod error;
pub mod game_host;
mod support;
pub mod website;

pub use cli::{CliConfig, CliFileParsed, config_path};
pub use env_names::ALL_ENV_VARS;
pub use error::ConfigError;
pub use game_host::GameHostConfig;
pub use website::{MachineProviderKind, ResolvedCoordinator, WebsiteConfig};
