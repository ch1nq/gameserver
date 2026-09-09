//! Shared environment variable name constants.
//!
//! The single source of truth for every `*_URL`, `PORT`, `ARENA_*`, `GAME_*`,
//! `MSB_*`, `REAPER_*`, `DOCKER_*`, and `ACHTUNG_*` name (fixes #11). All
//! crates must reference these instead of string literals so names cannot
//! drift between the website, coordinator, game host, and CLI.

/// Website server bind host.
pub const HOST: &str = "HOST";
/// Website server port / game-host listen port (shared name, different defaults).
pub const PORT: &str = "PORT";
/// Tracing filter.
pub const RUST_LOG: &str = "RUST_LOG";

/// GitHub OAuth client id (required).
pub const GITHUB_CLIENT_ID: &str = "GITHUB_CLIENT_ID";
/// GitHub OAuth client secret (required).
pub const GITHUB_CLIENT_SECRET: &str = "GITHUB_CLIENT_SECRET";

/// Postgres connection string (required).
pub const DATABASE_URL: &str = "DATABASE_URL";

/// RSA private key (PEM) used to mint registry JWTs (required).
pub const REGISTRY_PRIVATE_KEY: &str = "REGISTRY_PRIVATE_KEY";
/// JWT audience; must match the registry's `REGISTRY_AUTH_TOKEN_SERVICE`.
pub const REGISTRY_SERVICE: &str = "REGISTRY_SERVICE";
/// How the website reaches the registry API.
pub const REGISTRY_URL: &str = "REGISTRY_URL";
/// Host rendered into user-facing `docker login/tag/push` hints.
pub const REGISTRY_PUBLIC_HOST: &str = "REGISTRY_PUBLIC_HOST";

// ─── Coordinator ─────────────────────────────────────────────────────────────

/// Any truthy value enables the game coordinator; unset/false disables it.
/// Bare presence (empty string) counts as `true` for back-compat.
pub const ENABLE_COORDINATOR: &str = "ENABLE_COORDINATOR";
/// Which machine backend to use: `docker` or `microsandbox`.
pub const MACHINE_PROVIDER: &str = "MACHINE_PROVIDER";

/// Game host image (validated as an image URL).
pub const GAME_HOST_IMAGE: &str = "GAME_HOST_IMAGE";
/// Number of agents per game.
pub const AGENTS_PER_GAME: &str = "AGENTS_PER_GAME";
/// Game tick period in milliseconds.
pub const GAME_TICK_RATE_MS: &str = "GAME_TICK_RATE_MS";
/// Seconds to wait between games.
pub const GAME_INTERVAL_SECS: &str = "GAME_INTERVAL_SECS";
/// Seconds to keep retrying the first connection to a fresh game host.
pub const GAME_HOST_CONNECT_TIMEOUT_SECS: &str = "GAME_HOST_CONNECT_TIMEOUT_SECS";

/// Shared network that match containers attach to (docker backend, required).
pub const DOCKER_NETWORK: &str = "DOCKER_NETWORK";
/// Registry host the daemons pull private agent images from.
pub const DOCKER_REGISTRY_PULL_HOST: &str = "DOCKER_REGISTRY_PULL_HOST";
/// Prefix for spawned match/agent names (also the reaper default).
pub const AGENT_NAME_PREFIX: &str = "AGENT_NAME_PREFIX";

/// Per-machine vCPU limit (both backends).
pub const MACHINE_CPUS: &str = "MACHINE_CPUS";
/// Per-machine memory limit in MiB (both backends).
pub const MACHINE_MEM_MIB: &str = "MACHINE_MEM_MIB";

/// First host port of the microsandbox relay range.
pub const MSB_HOST_PORT_BASE: &str = "MSB_HOST_PORT_BASE";
/// Host address the agent relay ports bind to.
pub const MSB_HOST_BIND: &str = "MSB_HOST_BIND";
/// Pull private images over plain HTTP (local dev only).
pub const MSB_REGISTRY_INSECURE: &str = "MSB_REGISTRY_INSECURE";
/// Hard per-sandbox lifetime cap in seconds (optional).
pub const MSB_MAX_DURATION_SECS: &str = "MSB_MAX_DURATION_SECS";

/// How often the reaper scans, in seconds.
pub const REAPER_INTERVAL_SECS: &str = "REAPER_INTERVAL_SECS";
/// Age after which resources count as orphaned, in seconds.
pub const REAPER_MAX_AGE_SECS: &str = "REAPER_MAX_AGE_SECS";
/// Substring used to match this backend's resources for orphan cleanup.
pub const REAPER_PREFIX: &str = "REAPER_PREFIX";

// ─── Game host ───────────────────────────────────────────────────────────────

/// Arena width in game units.
pub const ARENA_WIDTH: &str = "ARENA_WIDTH";
/// Arena height in game units.
pub const ARENA_HEIGHT: &str = "ARENA_HEIGHT";

// ─── CLI ─────────────────────────────────────────────────────────────────────

/// CLI: API base URL (or `api_url` in `config.toml`).
pub const ACHTUNG_API_URL: &str = "ACHTUNG_API_URL";
/// CLI: numeric user id (or `user_id` in `config.toml`).
pub const ACHTUNG_USER_ID: &str = "ACHTUNG_USER_ID";
/// CLI: API token (or `api_token` in `config.toml`).
pub const ACHTUNG_API_TOKEN: &str = "ACHTUNG_API_TOKEN";
/// CLI: registry host users push to (or `registry_host` in `config.toml`).
pub const ACHTUNG_REGISTRY_HOST: &str = "ACHTUNG_REGISTRY_HOST";

/// gRPC port the game host listens on *inside* its machine.
/// Also the fallback dial port when a backend does not relay.
pub const DEFAULT_GAME_HOST_GRPC_PORT: u16 = 50051;
/// gRPC port agents listen on *inside* their machines.
pub const DEFAULT_AGENT_GRPC_PORT: u16 = 50052;

/// All flat env var names this crate consumes (used by the
/// `.env.example` coverage test so docs cannot drift from code).
pub const ALL_ENV_VARS: &[&str] = &[
    HOST,
    PORT,
    RUST_LOG,
    GITHUB_CLIENT_ID,
    GITHUB_CLIENT_SECRET,
    DATABASE_URL,
    REGISTRY_PRIVATE_KEY,
    REGISTRY_SERVICE,
    REGISTRY_URL,
    REGISTRY_PUBLIC_HOST,
    ENABLE_COORDINATOR,
    MACHINE_PROVIDER,
    GAME_HOST_IMAGE,
    AGENTS_PER_GAME,
    GAME_TICK_RATE_MS,
    GAME_INTERVAL_SECS,
    GAME_HOST_CONNECT_TIMEOUT_SECS,
    DOCKER_NETWORK,
    DOCKER_REGISTRY_PULL_HOST,
    AGENT_NAME_PREFIX,
    MACHINE_CPUS,
    MACHINE_MEM_MIB,
    MSB_HOST_PORT_BASE,
    MSB_HOST_BIND,
    MSB_REGISTRY_INSECURE,
    MSB_MAX_DURATION_SECS,
    REAPER_INTERVAL_SECS,
    REAPER_MAX_AGE_SECS,
    REAPER_PREFIX,
    ARENA_WIDTH,
    ARENA_HEIGHT,
    ACHTUNG_API_URL,
    ACHTUNG_USER_ID,
    ACHTUNG_API_TOKEN,
    ACHTUNG_REGISTRY_HOST,
];
