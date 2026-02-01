# Plan: Consolidate Overseer into Website

## Overview

Merge the overseer service functionality into the website binary to reduce operational complexity. The game/resource management logic will remain in the `agent-infra` crate as a library.

### Current State
- **Overseer**: Standalone gRPC service that provisions Fly.io infrastructure when agents are created
- **Website**: Calls overseer via gRPC for agent operations and image listing
- **Game Host**: Separate binary for running game simulations

### Target State
- **Website**: Handles all agent management (DB only) + runs game coordinator as background task
- **agent-infra**: Library crate providing machine provisioning abstraction
- **Game Host**: Spun up per-match by the coordinator, runs single game, destroyed after

---

## Architecture

```
Website
├── Web Routes (CRUD UI, watch games)
├── Agent Manager (DB operations only)
├── Registry Client (list images directly)
├── Game Coordinator (background task)
│   ├── Pick agents from roster
│   ├── Spawn game host + agent machines
│   ├── Start game via gRPC
│   ├── Poll for completion
│   ├── Record results
│   └── Destroy all machines
└── uses: agent-infra library

agent-infra (library)
├── MachineProvider trait
└── FlyMachineProvider impl

Per-Match Infrastructure (Fly.io)
├── Game Host (app+machine) - runs game logic
└── Agent Machines (app+machine each) - user containers
```

---

## Phases

### Phase 1: Restructure agent-infra as library ✅

Convert from gRPC service to library with machine provisioning abstraction.

- [x] Remove `main.rs` (gRPC server entrypoint)
- [x] Remove `server.rs` (gRPC trait impl)
- [x] Create `lib.rs` with public API
- [x] Define `MachineProvider` trait
- [x] Implement `FlyMachineProvider` using existing `fly_api.rs`
- [x] Update `Cargo.toml` - remove tonic server deps, configure as library

### Phase 2: Add registry client to website ✅

Website calls registry directly for listing images.

- [x] Copy/adapt registry client code to website
- [x] Update "new agent" page to use direct registry call
- [x] Remove gRPC `list_images` dependency

### Phase 3: Simplify agent management in website ✅

Agent CRUD becomes pure DB operations.

- [x] Remove `TournamentManagerClient` from App state
- [x] Remove `build.rs` proto compilation for tournament_manager
- [x] Remove tonic client deps from website
- [x] Simplify `create_agent` handler to DB insert only
- [x] Agent status remains `Active`/`Inactive` (roster membership only)

### Phase 4: Define game host gRPC interface ✅

Protocol for coordinator ↔ game host communication.

- [x] Create `protos/game_host.proto`
- [x] Create `protos/agent.proto` (game host ↔ agent communication)
- [x] Define `GameHost` service:
  - `StartGame(agents: [AgentEndpoint])` - begins a match
  - `GetStatus()` - returns game state (running/finished, results)

### Phase 5: Build game coordinator ✅

Separate crate for game coordination logic.

- [x] Create `libs/coordinator/` crate
- [x] Implement main game loop
- [x] Integrate `MachineProvider` for spawning
- [x] Implement gRPC client for game host
- [x] Implement status polling
- [x] Implement cleanup (destroy machines)
- [x] Implement `AgentRepository` trait for fetching agents
- [x] Integrate with website via `AgentManager`
- [x] Spawn coordinator in website (controlled by `ENABLE_COORDINATOR` env var)

### Phase 6: Update game host (achtung-host)

Game host implements the gRPC interface.

- [ ] Add `game_host.proto` compilation
- [ ] Implement `GameHost` gRPC service
- [ ] Accept agent endpoints, connect via gRPC
- [ ] Report game status and results

### Phase 7: Cleanup

- [ ] Delete `protos/tournament_manager.proto`
- [ ] Remove overseer Dockerfile and deployment config
- [ ] Update workspace configuration

---

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Coordinator location | Background task in website | Simpler than separate service |
| Machine lifecycle | App+machine per agent per match | Easier Fly.io networking |
| Game host lifecycle | New instance per match | Isolation, no state management |
| Coordinator ↔ Game host | gRPC with polling | Already have gRPC infrastructure |
| Image listing | Website calls registry directly | Removes unnecessary hop |

---

## Open Questions (to resolve during implementation)

- Agent selection strategy (random, round-robin, etc.)
- Number of agents per game
- Game host image location/deployment
- Results storage schema
- Observer WebSocket routing

---

## Files to Create/Modify

**Create:**
- `libs/agent-infra/src/lib.rs`
- `libs/agent-infra/src/provider.rs`
- `apps/website/src/coordinator/mod.rs`
- `apps/website/src/registry/client.rs` (or similar)
- `protos/game_host.proto`

**Modify:**
- `libs/agent-infra/Cargo.toml`
- `apps/website/Cargo.toml`
- `apps/website/src/web/app.rs`
- `apps/website/src/web/protected/agents.rs`
- `apps/achtung-host/` (gRPC server implementation)

**Delete:**
- `libs/agent-infra/src/main.rs`
- `libs/agent-infra/src/server.rs`
- `protos/tournament_manager.proto`
