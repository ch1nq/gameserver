//! Single source of truth for all API route paths.
//!
//! The server uses the `*_PREFIX` constants for `Router::nest()` and the
//! sub-route constants (e.g. `AGENTS`, `AGENT`) inside each nested router.
//!
//! The client combines prefix + sub-route via the helper functions below.

pub const AGENTS_PREFIX: &str = "/agents";
pub const TOKENS_PREFIX: &str = "/tokens";
pub const REGISTRY_PREFIX: &str = "/registry";

// Agents
pub const AGENTS: &str = "/";
pub const AGENT: &str = "/{id}";
pub const AGENT_ACTIVATE: &str = "/{id}/activate";
pub const AGENT_DEACTIVATE: &str = "/{id}/deactivate";

// Tokens
pub const TOKENS: &str = "/";
pub const TOKEN: &str = "/{id}";

// Registry
pub const IMAGES: &str = "/images";

pub fn agents_path() -> String {
    AGENTS_PREFIX.to_string()
}

pub fn agent_path(id: impl std::fmt::Display) -> String {
    format!("{}/{}", AGENTS_PREFIX, id)
}

pub fn agent_activate_path(id: impl std::fmt::Display) -> String {
    format!("{}/{}/activate", AGENTS_PREFIX, id)
}

pub fn agent_deactivate_path(id: impl std::fmt::Display) -> String {
    format!("{}/{}/deactivate", AGENTS_PREFIX, id)
}

pub fn tokens_path() -> String {
    TOKENS_PREFIX.to_string()
}

pub fn token_path(id: impl std::fmt::Display) -> String {
    format!("{}/{}", TOKENS_PREFIX, id)
}

pub fn images_path() -> String {
    format!("{}{}", REGISTRY_PREFIX, IMAGES)
}
