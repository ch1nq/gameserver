# Make the gRPC game host generic (live in arcadio), replacing the WebSocket host

## Context

We proved a full gRPC match runs end-to-end on the Firecracker host, but the prototype that got
us there is not mergeable as-is:

- `apps/game-host-grpc` is **100% hardcoded to Achtung** (imports `arcadio::games::achtung`,
  `map_direction`, `build_state` from `player_views()`). It threw away arcadio's existing *generic*
  host pattern — `GameServer<T: GameState>` in `libs/game-host/src/server.rs` drives **any** game
  over WebSocket with zero game-specific knowledge.
- The protos leak Achtung concepts into the *generic* orchestration layer: `game_host.proto`
  (`GameConfig.arena_width/height`) and the coordinator (`CoordinatorConfig.arena_width/height`,
  hardcoded `GAME=achtung`).

Goal: a generic gRPC host **inside arcadio** (mirroring how `GameServer<T>` was generic), so future
games plug in without touching the coordinator or the orchestration code.

Decisions:
- **Typed protobuf per game** — each game defines its own strongly-typed agent proto
  (Observation/Action); a small per-game adapter bridges proto ↔ engine. (Not opaque serde blobs.)
- **Replace the WebSocket host** — delete `server.rs`'s `GameServer<T>` and `apps/achtung-host`;
  gRPC becomes arcadio's only host transport.

## Design

Split the protocol into a **generic** coordinator↔host contract and a **per-game** host↔agent
contract:

- **`gamehost.proto` (generic, shared).** Coordinator ↔ game-host. `StartGame(agents, config)` /
  `GetStatus` → placements. Package renamed `achtung.gamehost` → **`gamehost`**. Drop
  `arena_width/height` from `GameConfig` (keep `tick_rate_ms`). This proto knows nothing about any
  game. Compiled by both the coordinator (client) and arcadio (server).
- **`achtung_agent.proto` (per-game, typed).** Game-host ↔ agents. This is today's `agent.proto`
  (typed `Position{x,y}`, `Direction{Straight,Left,Right}`, `PlayerState`) — kept as *Achtung's*
  agent proto (package `achtung.agent`). Compiled by arcadio (client) and `apps/sample-agent`
  (server). A future game ships its own `<game>_agent.proto`.

In arcadio, a generic host owns **all orchestration**; a per-game adapter owns **only the typed
bits**:

```
libs/game-host/src/grpc.rs
  ├─ pub trait GameAdapter                     // the per-game seam
  │    type Engine: GameState; type Client; type PlayerId: Eq+Hash+Clone;
  │    fn init_engine(&self, num_players) -> Engine;
  │    fn active_players(&self, &Engine) -> Vec<PlayerId>;   // for placement ordering
  │    async fn connect(&self, addr) -> Client;
  │    async fn initialize(&self, &mut Client, player_id, num_players);
  │    async fn get_action(&self, &mut Client, &Engine, &PlayerId) -> Engine::GameAction;
  │    async fn game_over(&self, &mut Client, placement);
  └─ pub struct GrpcGameServer<G: GameAdapter> // generic
       - implements the `gamehost` tonic service (StartGame/GetStatus)
       - session map Arc<Mutex<HashMap<game_id, Session>>>
       - the tick loop: connect+initialize all agents, per-tick get_action on each live player →
         handle_player_action → update_game_state → track elimination order (diff active_players) →
         get_game_result → placements (survivors first, then reverse elimination) → game_over
       - host tracks its own current_tick counter (no engine `tick()` needed)
```

The `GameState` trait is **unchanged** — all game-specific knowledge lives in the adapter, which is
exactly where the "typed per game" code belongs. Slot i ↔ agent_id[i] ↔ `get_player_ids()[i]`.

## Work items

### 1. Protos (`protos/`)
- `game_host.proto`: package `achtung.gamehost` → `gamehost`; remove `arena_width`, `arena_height`
  from `GameConfig`.
- Rename `agent.proto` → `achtung_agent.proto` (keep package `achtung.agent`, typed shape as-is).
  (`tournament_manager.proto` untouched — out of scope.)

### 2. arcadio generic host (`libs/game-host/`)
- `Cargo.toml`: add `tonic`, `prost`, `async-trait`, `serde_json` (already), and `tonic-build`
  (build-dep). **Remove** `warp`, `futures-util`, `pretty_env_logger` (WS-only).
- `build.rs` (new): compile `gamehost.proto` (server) + `achtung_agent.proto` (client).
- `src/grpc.rs` (new): `GameAdapter` trait + `GrpcGameServer<G>` (above). `tonic::include_proto!`
  for `gamehost` + `achtung.agent`.
- `src/games/achtung.rs` (or new `achtung_grpc.rs`): impl `GameAdapter` for an `AchtungGrpc`
  adapter — `init_engine` builds `Achtung` from an `AchtungConfig` it holds (arena default 1000²,
  env-overridable), `active_players`/`get_action` built from `player_views()`, Direction↔GameAction
  mapping. This is the single home for Achtung's gRPC knowledge.
- `src/lib.rs`: drop `mod server` / its re-exports; add `pub mod grpc`.
- **Delete** `src/server.rs`.

### 3. Delete the WebSocket binary
- Remove crate `apps/achtung-host` and its entry in the root `Cargo.toml` `members`.

### 4. Thin gRPC host binary (`apps/game-host-grpc/`)
- `main.rs` → ~15 lines: read `PORT` (+ optional arena env), construct `AchtungGrpc`, run
  `GrpcGameServer::new(adapter).serve(port)`.
- Remove its own `build.rs` + proto duplication (arcadio owns the host + protos now); `Cargo.toml`
  deps down to `arcadio` + `tokio` + `tracing`.

### 5. Sample agent (`apps/sample-agent/`)
- `build.rs`: compile `achtung_agent.proto` (was `agent.proto`).
- `main.rs`: unchanged behavior — dumb random typed `Direction` (already implemented). Keep.

### 6. De-Achtung the coordinator (`libs/coordinator/`)
- `build.rs`: compile `game_host.proto` (now package `gamehost`).
- `src/lib.rs`: `include_proto!("gamehost")`; remove `arena_width`/`arena_height` from
  `CoordinatorConfig` and the `StartGameRequest`/`GameConfig` it builds; drop the `GAME=achtung` /
  arena env on `spawn_game_host` (arena is now a game-host-side default/env). Keep `tick_rate_ms`.
- `examples/fc_match.rs`: drop the arena fields from `CoordinatorConfig`.

## Implementation order
1. Protos: rename `game_host.proto` package → `gamehost`, drop arena; rename `agent.proto` →
   `achtung_agent.proto`.
2. arcadio: deps + `build.rs`, `src/grpc.rs` (`GameAdapter` + `GrpcGameServer<G>`), lib wiring,
   delete `server.rs`.
3. `AchtungGrpc` adapter (the one home for Achtung's gRPC knowledge).
4. Delete WS binary `apps/achtung-host` (+ root `Cargo.toml` member).
5. Thin `apps/game-host-grpc` main; `apps/sample-agent` build.rs → `achtung_agent.proto`.
6. De-Achtung the coordinator (`include_proto!("gamehost")`, drop arena, drop `GAME=achtung`) +
   `examples/fc_match.rs`.
7. Verify: `cargo check --workspace` + clippy + agent-infra tests, then rerun the match on
   `hetzner-fc`.

## Verification
- `cargo check --workspace` + `cargo clippy` clean; `cargo test -p achtung-agent-infra` green.
- Confirm the coordinator crate compiles with **no** reference to arena/Position/Direction — the
  generic/game-specific boundary holds.
- On `hetzner-fc` (already installed, daemon up): rebuild both images
  (`achtung-game-host-grpc`, `achtung-sample-agent`), `docker save | firecracker-ctr -n achtung
  images import --snapshotter devmapper -`, then `cargo run -p coordinator --example fc_match` with
  the full `docker.io/library/...:latest` refs. Expect the same green result: 3 VMs, `Game started`
  → ticks → `Game finished` with a full placement order + winner, clean teardown, `rc=0`.

## Out of scope (unchanged)
Result persistence (`coordinator/src/lib.rs:138`), website/Postgres/OAuth path, Fly.io removal,
`tournament_manager.proto`.
