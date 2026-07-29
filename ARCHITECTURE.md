# Architecture

## Overview

Achtung Agents is a platform where developers build AI agents that compete in
"Achtung! Die Kurve" (Curve Fever). The system orchestrates matches by spinning up
ephemeral infrastructure (Fly.io machines or local firecracker microVMs), running a
game host alongside agent containers that communicate over gRPC, and streaming
results to spectators via gRPC-Web.

> **Note:** The project is transitioning from a WebSocket-based prototype to the
> gRPC-based architecture described here. The legacy WebSocket game host in
> `libs/game-host` still exists but is being replaced.

## High-Level Diagram

```
  ┌─────────┐  HTTP/HTML   ┌──────────────────────────────────┐
  │ Browser ├──────────────►│         Website (Axum)           │
  │         │◄──────────────┤                                  │
  │         │  gRPC-Web     │  ┌──────────┐  ┌─────────────┐  │
  │         │◄──────────────┤  │ REST API │  │ Registry    │  │
  └─────────┘  (live game   │  │ /api/v1  │  │ Token Auth  │  │
    stream)    │  └────┬─────┘  └──────┬──────┘  │
                       │       │      │          │             │
  ┌─────────┐  HTTP    │       │  ┌───┴──────────┴──────────┐  │
  │  CLI    ├──────────┘       │  │  libs/core (managers)   │  │
  │         │  Basic Auth      │  │  PostgreSQL via sqlx     │  │
  └─────────┘                  │  └─────────────────────────┘  │
                               │                               │
  ┌──────────┐  HTTP           │  ┌─────────────────────────┐  │
  │ Docker   ├─Bearer JWT─────►│  │  Coordinator (bg task)  │  │
  │ client   │                 │  └────────┬────────────────┘  │
  └────┬─────┘                 └───────────┼───────────────────┘
       │                                   │
       ▼                                   │  MachineProvider (Fly.io or Firecracker)
  ┌──────────┐                             │
  │ Docker   │                             ▼
  │ Registry │               ┌─────────────────────────────┐
  └──────────┘               │  Game Host + Agent machines  │
                             │                             │
                      gRPC   │  ┌───────────────────────┐  │
                 ┌──────────►│  │  Game Host (port 50051)│  │
                 │           │  └───┬───────┬───────┬───┘  │
                 │           │      │       │       │      │
                 │           │   gRPC (port 50052 each)    │
                 │           │      │       │       │      │
                 │           │  ┌───▼┐  ┌───▼┐  ┌───▼┐  │
                 │           │  │ A1 │  │ A2 │  │ A3 │  │  Agent containers
                 │           │  └────┘  └────┘  └────┘  │
                 │           └─────────────────────────────┘
                 │
          Coordinator connects
          to game host gRPC
```

## Machine Backends

The coordinator is generic over `MachineProvider` — the backend that provisions
and destroys machines. Two backends are implemented. Select with `MACHINE_PROVIDER`:

### Fly.io (`MACHINE_PROVIDER=fly`, default)

- Creates one ephemeral Fly app per match with a shared private IPv6 network
- Game host pulled from GHCR (public); agent images copied from the private registry
  to `registry.fly.io/{app}` using skopeo, then pulled by Fly machines
- Cleanup: delete the Fly app (cascades to all machines)

### Firecracker (`MACHINE_PROVIDER=firecracker`)

- Runs game host and agent containers as firecracker microVMs on the local host
- Uses containerd (`aws.firecracker` shim) and the devmapper snapshotter
- Images pulled directly from the private registry using scoped JWT tokens
- Network: per-match Linux bridge + TAP devices, strict iptables isolation
- Setup: see `scripts/setup-firecracker/README.md`

### MachineProvider trait (`libs/agent-infra`)

```rust
trait MachineProvider {
    type MatchContext;
    async fn init_match(match_id) -> MatchContext;      // allocate network, bridge
    async fn spawn(ctx, SpawnConfig) -> MachineHandle;  // start one container/VM
    async fn destroy(ctx, handle);                      // stop one container/VM
    async fn cleanup_match(ctx);                        // release network, bridge
    async fn list_orphaned(prefix, max_age) -> Vec<OrphanedResource>;
    async fn destroy_orphaned(resource);
}
```

The `slot` field in `SpawnConfig` identifies the machine's role:
- slot 0 → game host (IP: `subnet.1`, gRPC port 50051)
- slot 1+ → agents (IPs: `subnet.2`, `subnet.3`, etc., gRPC port 50052)

For firecracker, IPs are derived deterministically from the slot number within the
match's `/24` subnet. No shared mutable state is needed.

## Communication Protocols

### gRPC — Game Execution (protos/)

Three proto service definitions govern the game execution pipeline:

**`GameHost` service** (`game_host.proto`) — Called by the coordinator to control
a game. The coordinator sends `StartGame` with a list of `AgentEndpoint` addresses
and a `GameConfig` (tick rate, arena size), then polls `GetStatus` until the game
reaches `FINISHED` or `FAILED`. Returns `AgentPlacement` results with positions
and scores.

**`Agent` service** (`agent.proto`) — Called by the game host on each agent
container. `Initialize` sends the player ID, player count, and arena config.
`GetAction` is called every tick with the full `GameState` (all player positions,
directions, alive status) and expects a `Direction` response
(`STRAIGHT`/`TURN_LEFT`/`TURN_RIGHT`). `GameOver` notifies the agent of its final
placement.

**`TournamentManager` service** (`tournament_manager.proto`) — TODO: Agent
lifecycle management over gRPC. Currently, agent CRUD is handled via the REST API
instead.

Communication chain for a match:

```
Coordinator ──gRPC──► Game Host ──gRPC──► Agent containers
 (StartGame,          (port 50051)         (port 50052)
  GetStatus)           calls Initialize,
                       GetAction, GameOver
                       on each agent
```

### gRPC-Web — Live Spectating (TODO)

The game host will stream game state updates back to the website, which will
expose them to browser clients via gRPC-Web. This replaces the legacy WebSocket
observer path. The exact proto definition for spectator streaming is TBD.

### HTTP/REST — API & CLI

The website exposes a JSON REST API at `/api/v1` defined by the `GameApi` trait
in `libs/api-types`. The CLI (`apps/cli`) and any programmatic clients use this.

Routes are defined as constants in `libs/api-types/src/routes.rs` (single source
of truth for both server and client):

| Prefix       | Endpoints                                | Purpose               |
|--------------|------------------------------------------|-----------------------|
| `/agents`    | `GET /`, `POST /`, `DELETE /{id}`,       | Agent CRUD            |
|              | `POST /{id}/activate`, `POST /{id}/deactivate` |                |
| `/tokens`    | `GET /`, `POST /`, `DELETE /{id}`        | API token management  |
| `/registry`  | `GET /images`                            | List Docker images    |

Auth: HTTP Basic with `user-{id}:{api_token}`. Validated per-request by
bcrypt-comparing against the `api_tokens` table. No sessions.

### Docker Registry v2 Token Auth

Implements the [Docker Registry v2 token auth spec](https://docs.docker.com/registry/spec/auth/token/):

```
Docker client ──push/pull──► Registry ──401 + WWW-Authenticate──►
              ◄─────────────          ◄──────────────────────────
Docker client ──Basic Auth──► Website /registry/token
              ◄──JWT (RS256)─
Docker client ──Bearer JWT──► Registry  (authorized)
```

1. Registry rejects unauthenticated requests with a 401 pointing to the token endpoint
2. Docker client sends Basic auth (`user-{id}:{registry_token}`) to `GET /registry/token`
3. `libs/registry-auth` validates credentials, parses requested scopes
   (`repository:user-{id}/image:push,pull`), enforces namespace isolation
4. Returns a JWT signed with RS256. The `kid` header follows the libtrust spec
   (SHA256 of the DER-encoded public key, base32, colon-separated)
5. Docker client retries with the JWT as Bearer token

Namespace isolation: users can only access repositories under `user-{id}/`.

## Game Match Lifecycle

The coordinator (`libs/coordinator`) runs as a `tokio::spawn` background task in
the website process when `ENABLE_COORDINATOR` is set. It loops:

```
1. Select agents    ── AgentRepository::get_random_active_agents(4)
                       (random active agents from PostgreSQL)

2. Init match       ── MachineProvider::init_match(match_id)
                       Fly:         create app + private IPv6 network
                       Firecracker: allocate /24 subnet, create Linux bridge,
                                    set up iptables isolation rules

3. Spawn game host  ── MachineProvider::spawn(ctx, slot=0)
                       Public image from GHCR, no registry auth needed

4. Spawn agents     ── For each agent (slot=1,2,3,...):
                       a. Get scoped JWT with pull-only access (DeployTokenProvider)
                       b. MachineProvider::spawn(ctx, slot=N)
                          Fly:         copy image via skopeo, create Fly machine
                          Firecracker: pull image (ctr) → create TAP → CreateVM
                                       (static IP on the TAP) → ctr run w/ the
                                       aws.firecracker.vm.id annotation

5. Run game         ── gRPC: Connect to game host on {private_ip}:50051
                       Send StartGame with agent endpoints ({ip}:50052)
                       Poll GetStatus every 1s until FINISHED or FAILED

6. Destroy machines ── MachineProvider::destroy(ctx, handle) for each machine
                       Fly:         no-op per machine (app teardown handles it)
                       Firecracker: delete container/task, StopVM, delete TAP

7. Cleanup match    ── MachineProvider::cleanup_match(ctx)
                       Fly:         delete Fly app (cascades to all machines)
                       Firecracker: remove iptables rules, delete bridge,
                                    release subnet back to pool

8. Record results   ── TODO: persist GameResult to database
                       Sleep game_interval (default 10s), repeat
```

### Orphan cleanup (Reaper)

A background `Reaper<P>` task (sharing the same `Arc<P>` as the coordinator) runs
every `REAPER_INTERVAL_SECS` (default 5 min). It calls `list_orphaned` to find
stale resources older than `REAPER_MAX_AGE_SECS` (default 1 hour) and destroys
them. This handles crashes during a match without leaking infrastructure.

### Firecracker network isolation

For each match, the firecracker backend creates:

```
Host bridge: br-m-{match}  (10.200.X.254/24)  ← coordinator connects here
  ├── tap-m-{match}-0  →  game host microVM   (10.200.X.1)  port 50051
  ├── tap-m-{match}-1  →  agent 1 microVM     (10.200.X.2)  port 50052
  └── tap-m-{match}-2  →  agent 2 microVM     (10.200.X.3)  port 50052
```

iptables rules enforce:
- Game host ↔ agents: **allowed**
- Agent → agent: **dropped**
- Any → internet: **dropped** (no NAT, no default route)

Guest networking is applied via the firecracker-containerd **CreateVM** control
API (`fccontrol` service), which binds the pre-created host TAP to the microVM
and passes a static `IPConfiguration` so the in-VM agent configures the guest
NIC. This is **not** done via OCI spec annotations. The agent↔agent DROP rules
only apply to intra-bridge traffic when `br_netfilter` is loaded and
`net.bridge.bridge-nf-call-iptables=1` (configured by `install.sh`).

Subnets are allocated from a `/16` pool (`FIRECRACKER_SUBNET_POOL`, default
`10.200.0.0/16`) supporting up to 255 concurrent matches.

## Auth Systems

Three independent authentication mechanisms:

| Context              | Method                     | Implementation                  |
|----------------------|----------------------------|---------------------------------|
| Browser → Website    | GitHub OAuth → session cookie | `axum-login` + `tower-sessions` (PostgreSQL session store) |
| CLI/API → `/api/v1`  | HTTP Basic Auth            | `ApiAuth` extractor in `libs/api`, bcrypt verify against `api_tokens` table |
| Docker → Registry    | Basic Auth → JWT (RS256)   | `libs/registry-auth`, scoped to `user-{id}/` namespace |

The REST API (`/api/v1`) sits outside the session middleware layer — it uses
stateless Basic auth only. The registry token endpoint (`/registry/token`) sits
inside the session layer but performs its own Basic auth extraction internally.

**System tokens**: For internal operations (listing catalog, generating deploy
tokens for the coordinator), `RegistryTokenManager` generates JWTs directly
without going through the HTTP token endpoint. These are cached in memory via
`Arc<RwLock<Option<RegistryJwtToken>>>`.

## State & Data Access

### Website Process State

Two Axum state structs share the same underlying `PgPool` (which is `Arc`-based):

- **`AppState`** — Used by HTML web UI routes (agent manager, token managers, registry client)
- **`ApiState`** — Used by REST API routes (same managers + user manager)

All managers are `Clone` (cheap — they wrap `PgPool`). The coordinator receives
trait objects (`Box<dyn AgentRepository>`, `Box<dyn DeployTokenProvider>`) to stay
decoupled from `libs/core`.

### Database Access Boundaries

Only `libs/core` managers should execute SQL queries. The one exception is the
`axum-login` `Backend` in `apps/website/src/users.rs` which directly queries the
`users` table for OAuth upsert.

```
libs/core managers:
  UserManager         → users table
  AgentManager        → agents table
  ApiTokenManager     → api_tokens table
  RegistryTokenManager → registry_tokens table

PostgresStore (sessions) → internal session tables
```

Other crates access the database indirectly through trait objects (e.g., the
coordinator uses `AgentRepository`, not `AgentManager` directly).

## Legacy: WebSocket Game Host

> Being replaced by the gRPC architecture described above.

The current `libs/game-host` (`arcadio` crate) implements a Warp + WebSocket
game server. Agents connect to `ws://host/join/player`, observers to
`ws://host/join/observer`. Messages are JSON over WebSocket binary frames.

Server → Client events: `AssignPlayerId`, `InitialState`, `UpdateState` (diff),
`GameOver`. Client → Server: `Action` (Left/Right/Forward), `RequestUpdate`.

Two tick modes exist: server-driven (fixed `tick_rate_ms`) and client-driven
(tick on `RequestUpdate` — useful for RL training).

The Python SDK (`sdk/python`, package `arcadio-client`) connects to this
WebSocket server. Agent developers implement the `Strategy` protocol
(`take_action(game_state, player_id) → GameAction | None`) and run via
`GameClient.connect().run()`. A `SlowStrategy` wrapper offloads compute-heavy
strategies to a thread pool.

## Crate Dependency Graph

```
apps/website ─► libs/api ─► libs/api-types ─► libs/common
     │              │
     │              ▼
     ├────────► libs/core (DB, managers)
     │              │
     │              ▼
     │         libs/registry-auth
     │
     ├────────► libs/coordinator ─► libs/agent-infra (Fly.io + Firecracker)
     │              │
     │              ▼
     │         libs/common (traits: AgentRepository, DeployTokenProvider)
     │
     └────────► libs/ui (Maud HTML components)

apps/achtung-host ─► libs/game-host (arcadio, legacy WebSocket engine)

apps/cli ─► libs/api-types (HttpClient implements GameApi)
```

Key design principle: the `coordinator` crate is generic over `MachineProvider`
and depends on `common` for trait definitions and `agent-infra` for machine
provisioning, but never on `core` directly. This keeps the match orchestration
layer decoupled from the database layer.
