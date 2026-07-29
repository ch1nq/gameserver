# AGENTS.md

## Project Overview

Achtung Agents — a platform where developers build autonomous AI agents that compete
in "Achtung! Die Kurve" (Curve Fever). Rust workspace monorepo with `apps/` (binaries)
and `libs/` (shared crates), plus a Python SDK.

See [ARCHITECTURE.md](ARCHITECTURE.md) for system architecture, communication
protocols, data flows, and the game match lifecycle.

## Architecture

- **apps/website** — Main web app (Axum, Maud HTML, GitHub OAuth, REST API)
- **apps/achtung-host** — Game host server (Warp + WebSocket real-time game engine)
- **apps/cli** — CLI tool (clap derive)
- **libs/core** — Business logic, DB (sqlx + PostgreSQL), migrations
- **libs/api** — Axum REST API route handlers
- **libs/api-types** — Shared API contract types + HTTP client (used by server and CLI)
- **libs/common** — Shared primitive types (IDs, newtypes, traits)
- **libs/coordinator** — Game match orchestration via gRPC
- **libs/agent-infra** — Fly.io machine provisioning for game matches
- **libs/game-host** — Game engine library (`arcadio` crate)
- **libs/registry-auth** — Docker Registry v2 token auth (JWT, RSA)
- **libs/ui** — Maud HTML UI components
- **libs/ranking** — Ranking algorithms (placeholder)
- **protos/** — Protobuf definitions for gRPC services
- **sdk/python** — Python SDK (`arcadio-client`)

## Build / Lint / Test Commands

### Rust

```bash
# Build everything
cargo build

# Build a specific crate
cargo build -p achtung-core
cargo build -p api

# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p registry-auth

# Run a single test by name
cargo test -p registry-auth -- test_parse_scopes

# Run a single test with output visible
cargo test -p registry-auth -- test_parse_scopes --nocapture

# Format all code (enforced by pre-commit hook)
cargo fmt --all

# Check formatting without modifying
cargo fmt --all -- --check

# Lint with clippy
cargo clippy --workspace

# Check compilation without building (fast)
cargo check --workspace
```

### Docker / Local Dev

```bash
# Start local Postgres + registry + website
docker compose up

# Build a Docker image for an app
just build website
just build achtung-host

# Compile protobuf definitions
just compile-protos
```

### Python SDK (in sdk/python/)

```bash
# Format/lint with ruff
ruff check .
ruff format .
```

### Environment Setup

Copy `.env.example` to `.env` and fill in secrets. Required vars:
`GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `REGISTRY_PRIVATE_KEY`, `REGISTRY_CERT`.
Database URL for local dev: `postgresql://arcadio:arcadio@localhost:5432/arcadio`.

## Code Style Guidelines

### Rust Edition

Most crates use **edition 2024**. The game-host crates (`achtung-host`, `arcadio`)
use **edition 2021**. Do not change editions without good reason.

### Imports

- All `use` statements at the top of the file, one item per `use` line
- Let `cargo fmt` handle sorting (alphabetical)
- Use nested braces for multiple items from the same crate: `use axum::{Json, Router};`
- No manual blank-line grouping required — `cargo fmt` handles it

### Naming Conventions

| Element            | Convention            | Example                                      |
|--------------------|-----------------------|----------------------------------------------|
| Variables/fields   | `snake_case`          | `db_pool`, `agent_id`, `token_hash`          |
| Functions/methods  | `snake_case`          | `create_agent`, `validate_token`             |
| Structs/Enums      | `PascalCase`          | `AgentManager`, `ApiTokenError`              |
| Enum variants      | `PascalCase`          | `Active`, `TokenLimitReached`                |
| Constants          | `SCREAMING_SNAKE_CASE`| `MAX_TOKENS_PER_USER`, `BCRYPT_COST`         |
| Type aliases       | `PascalCase`          | `type UserId = i64;`                         |
| Modules/files      | `snake_case`          | `api_tokens.rs`, `fly_api.rs`                |
| Crate names        | `kebab-case` (Cargo)  | `agent-infra`, `registry-auth`               |

### Type Patterns

- **ID types** are aliases: `pub type UserId = i64;` (in `libs/common/src/ids.rs`)
- **Domain values** use newtype wrappers with validation in `FromStr`:
  `AgentName(String)`, `ImageUrl(String)`, `PlaintextToken(String)`
- Implement `Deref`, `AsRef<str>`, `FromStr` on newtypes as needed
- Derive `serde::Serialize, serde::Deserialize` inline (not via use import)
- Use `#[derive(Debug, Clone, FromRow, serde::Serialize)]` on DB-mapped structs
- Use `From` impls to convert between domain models and API types

### Error Handling

- Define error enums with `#[derive(thiserror::Error)]` for library crates
- Each error variant has `#[error("message")]` for Display
- Implement `IntoResponse` on API error types to map to HTTP status codes
- Use `.map_err(|e| ApiError::Internal(e.to_string()))?` at API boundaries
- Top-level binaries may use `Result<(), Box<dyn std::error::Error>>`
- Never use `.unwrap()` in library/production code

### Async Patterns

- All I/O functions are `async fn`, runtime is Tokio (`#[tokio::main]`)
- Use `#[async_trait::async_trait]` for async trait methods (older crates)
- Use `#[trait_variant::make(Send)]` for async traits (newer crates)
- Background tasks via `tokio::spawn`

### Database (sqlx + PostgreSQL)

- Compile-time checked queries: `sqlx::query!()` and `sqlx::query_as!()`
- Raw SQL in `r#"..."#` string literals
- Type overrides for enums: `status as "status: AgentStatus"`
- Manager pattern: structs wrapping `PgPool` with domain methods
- Migrations live in `libs/core/migrations/` (plain SQL, timestamped filenames)
- Connection setup in `libs/core/src/db.rs` via `connect_and_migrate()`

### Web / API Patterns (Axum)

- Route handlers are standalone `async fn` (not methods), private to module
- Extractors: `State(state)`, `Path(id)`, `Json(body)`, custom `ApiAuth(user_id)`
- Each API module exposes a `pub fn router() -> Router<ApiState>` builder
- Route paths defined as constants in `libs/api-types/src/routes.rs`
- HTML rendering via `maud` crate (type-safe, not templates)
- Auth: GitHub OAuth for browser sessions, Basic auth for API tokens

### Logging

- **Newer crates**: use `tracing` — `tracing::info!()`, `tracing::error!()`, etc.
- **Game-host crates**: use `log` — `log::info!()`, `log::warn!()`
- Use structured fields: `tracing::info!(agent_id = id, "Created agent")`
- New code should use `tracing`, not `log`

### File Organization

Typical source file structure (top to bottom):
1. Module-level doc comment (`//!`)
2. `use` imports
3. Type definitions (structs, enums, constants)
4. `impl` blocks — constructor (`fn new`) first, then public, then private
5. Trait implementations (`From`, `IntoResponse`, etc.)
6. `#[cfg(test)] mod tests` at the bottom

Module root files (`lib.rs`, `mod.rs`) declare submodules and re-export with globs:
```rust
pub mod agent;
pub use agent::*;
```

### Comments and Documentation

- Module-level: `//!` doc comments with description and links
- Item-level: `///` doc comments, typically 1-2 sentences
- Inline: `//` comments explaining "why", not "what"
- Reference URLs for specs/algorithms in doc comments

### Pre-commit Hooks

Enforced via `prek` (see `.pre-commit-config.yaml`):
- Trailing whitespace removal
- End-of-file newline fixer
- YAML/TOML validation
- `cargo fmt --all` on all Rust files

### Files to Never Commit

- `.env` (secrets) — use `.env.example` as reference
- `*.pem` (crypto keys)
- `target/` (build artifacts)
- `CLAUDE.md` (gitignored, personal agent notes)
