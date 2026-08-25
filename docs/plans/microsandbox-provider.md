# Plan: microsandbox `MachineProvider`

Status: **not started** — Phase 0 verification is a go/no-go gate; keep the
checkboxes below up to date as phases land.

## Why

`libs/agent-infra` currently has one provider, `DockerMachineProvider`, with two
isolation modes (`libs/agent-infra/src/docker.rs`): `SharedNetwork` for local dev
and `PerMatchNetworks` for production gVisor. Both share the host kernel — gVisor
interposes a userspace kernel, but there is no hardware boundary.

[microsandbox](https://docs.microsandbox.dev/) runs each workload as a real
microVM with its own guest Linux kernel. For untrusted user-submitted agent
images that is a stronger boundary than `runsc`. It also ships a first-party Rust
SDK (`microsandbox` on crates.io, 0.6.x), so the provider is a library
integration rather than another hand-rolled control path — explicitly the failure
mode that killed the Firecracker backend (see `gvisor-migration.md`).

This is a **production-candidate** provider, not a spike: it is wired into
`MACHINE_PROVIDER`, supports private agent image pulls, and is reaped.

### Accepted regression

microsandbox requires **KVM** on Linux. `gvisor-migration.md` lists "requires
KVM-capable hardware" as a reason for abandoning Firecracker; this provider
re-introduces that constraint. Accepted deliberately. Consequence: the provider
cannot run from the website container as currently configured
(`docker-compose.yml`) without `/dev/kvm` passthrough plus the msb runtime inside
that image — see Phase 4.

## Target architecture

microsandbox gives every sandbox its own `/30` from `172.16.0.0/12` with a
host-side gateway and a userspace netstack. **There is no shared-L2 primitive**,
so the per-slot-bridge topology from `gvisor-migration.md` has no equivalent
here: no set of sandboxes can be placed on one segment.

All match traffic therefore relays through the host:

```
coordinator (host process)
    |  127.0.0.1:{base+0}                published port
    v
game host sandbox (slot 0)
    |  egress: allow_host (narrowed to relay ports) + DNS
    |  host.microsandbox.internal:{base+n}
    v
agent n sandbox (slot n)
    egress: default deny, ZERO rules
```

This works because the protocol is one-directional: the coordinator dials the
game host, the game host dials agents (`libs/coordinator/src/lib.rs`,
`libs/game-host/src/games/achtung_grpc.rs`), and agents never dial out.

Isolation properties and how they are enforced:

| Property | Mechanism |
|---|---|
| coordinator -> game host | published port bound on `127.0.0.1` |
| game host -> agent | `Host` group, narrowed to the agent relay ports |
| agent -> agent | agent has **no egress rules at all**, so it cannot reach the host relay that fronts other agents |
| agent -> internet | `default_egress: Deny` |
| agent -> host services | same: no egress rules, so no gateway access |
| kernel isolation | real microVM with its own guest kernel (stronger than `runsc`) |
| CPU/memory abuse | `cpus` / `memory` caps per sandbox |
| fork bombs | **not covered** — no `pids_limit` equivalent; memory cap is the only bound. Deferred, see Hardening. |

Agent isolation is structural rather than configured: the policy is
deny-by-default with an empty rule list, so there is no rule to get wrong. It
fails closed.

## Load-bearing design decisions (do not silently change these)

1. **The host relay is mandatory, not a shortcut.** There is no guest-to-guest
   path in microsandbox. Any future attempt to remove the relay has to change
   transport (see Hardening: vsock), not just policy.
2. **`create()` does not run the image workload.** The microsandbox docs are
   explicit: "Sandbox creation is boot-only: configuring an image, ENTRYPOINT, or
   CMD does not execute that workload." `create()` boots the VM and starts the
   guest agent only. The provider MUST call `exec_default()` (which resolves the
   effective `ENTRYPOINT + CMD`) to start the game host / agent process, and MUST
   NOT block on it — the workload runs for the whole match. This differs from
   Docker, where `start_container` runs the image CMD and that is the entire
   machine. Getting this wrong produces a silent "connection refused after 5s"
   in the coordinator.
3. **Sandboxes are created detached.** `MachineHandle` carries strings only, no
   live SDK object, and detached sandboxes survive a coordinator crash so the
   reaper can collect them. `destroy` re-acquires by name via `Sandbox::get`.
4. **Host ports are `port_base + slot`, with no allocator.** One match runs at a
   time (`libs/coordinator/src/lib.rs`: "One game runs at a time, so a single
   slot suffices"). A leftover sandbox from a crashed run holding a port is
   handled by a pre-flight sweep in `init_match` reusing the reaper's own
   `list_orphaned` / `destroy_orphaned` code — one mechanism, two callers.
   Concurrent matches would require a real allocator; that is a follow-up.
5. **Private pulls use `system` + the deploy JWT as a Basic password, and the
   token endpoint returns it verbatim.** See "Registry authentication" below.
   `user_has_access` for `System` returns `false` so the system principal can
   never mint anything — if the passthrough branch is ever bypassed, the pull
   fails closed.
6. **`MachineHandle.private_ip` is consumer-relative.** For this backend it is
   `127.0.0.1` for slot 0 (read by the coordinator, on the host) and
   `host.microsandbox.internal` for agents (read by the game host, inside a
   guest). The field means "the address by which *this machine's consumer*
   reaches it", not "this machine's own IP". Needs a doc comment on the field.
7. **`num_slots` is unused by this backend.** It exists because gVisor must
   pre-create every network before any container starts. Recorded so nobody
   "cleans it up".

## Registry authentication

microsandbox's `RegistryAuth` has exactly one variant, `Basic { username,
password }` — there is no bearer/identity-token option. The coordinator holds a
pre-minted JWT from `get_system_deploy_token_for`
(`libs/core/src/registry/manager.rs`), scoped to exactly one repository with
`pull` only, which it currently hands to bollard as `registrytoken`
(`libs/agent-infra/src/docker.rs`).

`token_handler` (`libs/registry-auth/src/auth.rs`) is already a spec-compliant
Docker v2 token endpoint that reads Basic auth. Two gates block `system`:

- `parse_user_id("system")` returns `None` (requires the `user-N` prefix).
- `is_valid_token` bcrypt-verifies against the `registry_tokens` table; the
  coordinator has no row there.

The fix is **passthrough**, not minting: for the `System` principal, verify the
presented JWT's signature and return it unchanged. The JWT is already a valid
registry bearer token — that is exactly how the bollard path works today.

```
msb  -> 401 from registry, WWW-Authenticate: realm=/registry/token,
                           scope=repository:user-5/bot:pull
msb  -> GET /registry/token   Authorization: Basic system:<deploy-jwt>
us   -> verify signature, echo the JWT back unchanged
msb  -> Authorization: Bearer <jwt>  -> registry enforces its access claim
```

Why passthrough over an intersection check:

| | mint + intersect scopes | passthrough |
|---|---|---|
| scope amplification | prevented, if the comparison is right | **structurally impossible** — nothing is minted |
| scoping authority | split across two crates | single: `get_system_deploy_token_for`, unchanged |
| enforcement point | our code | the registry, same as today |
| trait churn | `user_has_access` + `validate_for_user` signatures | one additive method with a default impl |

Details that matter: verify the signature even though we only echo it (so the
endpoint 401s at the right layer rather than handing back arbitrary strings), and
compute `expires_in` from the JWT's real `exp` rather than a fresh 30 minutes, so
msb does not cache past expiry.

**Unverified:** whether msb performs the Basic -> realm exchange at all. The docs
only say `auth()` "set[s] explicit credentials for the image registry", and
microsandbox ships its own `microsandbox-image` crate. A spec-compliant OCI
client follows the `WWW-Authenticate` realm, but this is not documented. If it
sends Basic directly to the registry instead, `registry:2` under
`REGISTRY_AUTH: token` rejects it and no work on our side helps — fallback would
be `docker save | msb load`, which needs Docker alongside msb and a copy per
spawn. Phase 0 settles this first because it is cheap and reshapes the plan.

## Phases

### Phase 0 — verification (go/no-go, before any provider code)

Throwaway example, deleted afterwards. Ordered cheapest-and-most-decisive first.
Items 2, 3 and 4 are independent kill switches.

- [ ] 1. `/dev/kvm` present; `setup::is_installed()` / `setup::install()` works.
- [ ] 2. **Private pull**: `Basic system:<deploy-jwt>` + `registry(|r| r.insecure())`
      reaches our realm and pulls from `localhost:5001`. Confirms the challenge
      scope matches what `get_system_deploy_token_for` mints (`repository()` on
      `AgentImageUrl` yields `user-{id}/{repo}`, no tag — the correct Docker
      scope form).
- [ ] 3. **Workload actually runs**: `create()` then `exec_default()` starts a
      listener on a published port, reachable from the host. Confirms decision 2.
- [ ] 4. **Relay topology**: sandbox A with `.port(hostA, 50051)` and egress deny
      + DNS + host; sandbox B with `.port(hostB, 50052)` and egress deny, no
      rules. Assert: host -> `127.0.0.1:hostA` open; A ->
      `host.microsandbox.internal:hostB` open; **B ->
      `host.microsandbox.internal:hostA` blocked**; B -> `1.1.1.1:443` blocked.
- [ ] 5. Confirm published ports require `default_ingress: Allow`.
- [ ] 6. Check whether the gateway IP can be resolved numerically at spawn time,
      so slot 0 might not need DNS egress at all.
- [ ] 7. Record msb version, kernel version and all results in the Verification
      log below.

Two ways item 4 kills the design: if a guest cannot reach a host `127.0.0.1`
listener through `host.microsandbox.internal` there is no relay; if an
egress-denied sandbox *can* reach it anyway, agent isolation is gone with no
second lever available.

### Phase 1 — trait + coordinator

- [ ] `SpawnConfig` gains `grpc_port: u16` — the port the process listens on
      *inside* the machine. Docker ignores it; microsandbox needs it for
      `port(host, guest)`.
- [ ] `MachineHandle` gains `grpc_port: Option<u16>`. Docker sets `None`;
      microsandbox sets `Some(base + slot)`.
- [ ] Coordinator passes the ports into `SpawnConfig`, and at the two dial sites
      uses `handle.grpc_port.unwrap_or(config.game_host_grpc_port)` /
      `.unwrap_or(config.agent_grpc_port)`.
- [ ] Doc comment on `private_ip` per decision 6.
- [ ] Update `docker.rs` and `examples/gvisor_smoke.rs` for the new fields.
- [ ] `cargo check` + existing tests pass.

### Phase 2 — registry system principal

- [ ] `RegistryPrincipal { System, User(UserId) }` as `RegistryAuth::UserId`.
- [ ] `parse_user_id`: `"system"` -> `System`; `user-N` -> `User(N)`; else `None`.
- [ ] `is_valid_token`: `System` -> verify RS256 signature, `aud`, `iss`, `exp`;
      `User` -> existing bcrypt path, untouched.
- [ ] `user_has_access`: `System` -> `false` (decision 5); `User` -> existing
      namespace check, untouched.
- [ ] Additive `passthrough` trait method with a `None` default, so
      `TestRegistryAuth` needs no change. `RegistryTokenManager` returns the
      verified JWT for `System`.
- [ ] `RegistryAuthConfig` caches the public key PEM at construction, next to
      `signing_key` (`key_id_from_pem` already derives it).
- [ ] `token_handler` consults `passthrough` after `is_valid_token`, before scope
      parsing; `expires_in` from the JWT's own `exp`.
- [ ] Tests: valid deploy JWT passes through with scope intact; expired rejected;
      foreign-signed rejected; `System` mints nothing; user path unregressed.

No new env vars, no compose changes.

### Phase 3 — the provider

New `libs/agent-infra/src/microsandbox.rs`, re-exported from `lib.rs`.
Deliberately mirrors `docker.rs` structure so the two read the same way.

```rust
pub struct MicrosandboxProviderConfig {
    pub cpus: u8,
    pub memory_mib: u32,
    pub host_port_base: u16,
    pub registry_pull_host: String,
    pub registry_insecure: bool,
    pub max_duration_secs: Option<u64>,
}
```

- [ ] `MatchContext { match_id: String }`. `init_match` records the id and runs
      the pre-flight sweep (decision 4).
- [ ] `spawn`: `Sandbox::builder("achtung-{match}-slot-{n}")` + image (prefixed
      with `registry_pull_host` for `Private`) + `registry(|r| r.auth(Basic{
      username: "system", password: token }).insecure())` +
      `pull_policy(IfMissing)` + `cpus` / `memory` + env from `SpawnConfig` +
      labels `achtung.match` / `achtung.created_at` + `port(base + slot,
      config.grpc_port)` + slot-dependent network policy + `max_duration` +
      `.replace()` + `.detached(true)` + `.create()`, then `detach()`.
- [ ] `spawn` then calls `exec_default` **without blocking** (decision 2):
      `exec_default_stream()` and drop the handle, or a detached task. Return
      once the workload is launched, not when it exits. A `NoDefaultCommand`
      error maps to `MachineError::MachineCreation`.
- [ ] `policy_for_slot`: slot 0 gets `default_deny` + DNS +
      `allow_host()` **narrowed to the agent relay port range**
      (`egress(|e| e.tcp().ports(..).allow_host())`) so the game host cannot
      reach Postgres, the registry or `/registry/token`. Slots 1+ get
      `default_deny` with no rules and `default_ingress: Allow` for the
      published port.
- [ ] `destroy`: `Sandbox::get(name)` -> `stop()` -> `remove()`, tolerating
      not-found (same shape as `is_not_found` in `docker.rs`).
- [ ] `cleanup_match`: no shared resources; log and return `Ok`.
- [ ] `list_orphaned`: paginate `Sandbox::list_with(|l| l.label("achtung.match",
      ..))`, filter on `name().starts_with(prefix)` and `created_at()` older than
      `max_age`. Only ever `OrphanKind::Machine` — this backend has no networks.
- [ ] `destroy_orphaned`: stop + remove by name.
- [ ] Errors map onto existing `MachineError` variants: image -> `ImageCopy`,
      create -> `MachineCreation`, stop/remove -> `Destruction`.
- [ ] Dependency: `microsandbox = { version = "0.6", default-features = false,
      features = ["net", "prebuilt"] }`. Dropping default `keyring` keeps `dbus`
      out of the server image. It has a build script and prebuilt runtime
      artifacts, so confirm a clean `cargo build` early rather than at the end.

`achtung.` label keys are safe; microsandbox reserves only `sandbox.`,
`microsandbox.` and `service.`.

### Phase 4 — wiring

- [ ] `app.rs`: `"microsandbox"` arm plus `microsandbox_config_from_env()`,
      reusing `MACHINE_CPUS` / `MACHINE_MEM_MIB` and adding
      `MSB_HOST_PORT_BASE`, `MSB_REGISTRY_INSECURE`. Reaper prefix `"achtung-"`,
      same as the other two backends.
- [ ] Guard provider construction on `is_installed()` so a missing runtime fails
      at startup rather than mid-match.
- [ ] `.env.example` section documenting the `/dev/kvm` requirement and that
      images come from the msb cache or a registry.
- [ ] Decide the compose story: running this from the website container needs
      `/dev/kvm` passthrough *and* the msb runtime in that image, versus today's
      mounted Docker socket. Recommendation: keep `MACHINE_PROVIDER=docker` in
      `docker-compose.yml` for local dev and drive this backend from a host-run
      website process. Decide before wiring so it is not bolted on late.

### Phase 5 — validation

- [ ] Unit tests on the pure parts: sandbox/label naming, `host_port(slot)`,
      `policy_for_slot(0)` vs `policy_for_slot(n)` shape, image-ref construction.
- [ ] `libs/agent-infra/examples/microsandbox_smoke.rs`, reusing the `Checks`
      struct and isolation matrix from `gvisor_smoke.rs`, probing via
      `Sandbox::get(name)` + `exec` instead of `docker exec`.
- [ ] Real local match: game host image into the local registry (or `docker save
      | msb load`), `MACHINE_PROVIDER=microsandbox`, spectator stream visible at
      `localhost:3000`.
- [ ] Hostile-agent image from `gvisor-migration.md` Phase 4 re-run against this
      backend once it exists.

## Hardening (deferred, deliberately)

Recorded so these are choices rather than oversights.

- **Fork-bomb containment.** No `pids_limit` equivalent; the memory cap is the
  only bound. Investigate guest rlimits (the SDK exposes `rlimit` per exec, and
  the changelog mentions guest rlimits) as the likely mechanism.
- **vsock transport.** Would let agents run with `disable_network()` — no virtual
  NIC at all, structural rather than policy isolation. Explored and deferred:
  vsock is **guest <-> host only** (guest connects to host CID 2), so it does not
  remove the host relay, it only swaps loopback TCP for a Unix socket on the same
  hop. Cost: the protocol direction is wrong (the game host dials agents, vsock
  is guest-initiated), so it needs a reverse tunnel plus a per-match host relay
  daemon; and it needs a static musl shim injected into every agent image via
  `patch()` to proxy AF_VSOCK <-> `127.0.0.1:50052`, since neither tonic nor
  grpcio speaks AF_VSOCK. Whether a guest can *accept* inbound vsock is
  undocumented and decides which of the two shapes applies.
  Encouraging: `connect(&self, address: &str)` in
  `libs/game-host/src/grpc.rs` is already a transport seam, so this plugs in
  without touching game logic.
- **System-principal scope narrowing.** Passthrough already makes amplification
  impossible, so there is nothing outstanding here — noted only to record that
  the alternative (mint + intersect requested scope against the presented JWT's
  own `access` claim) was considered and rejected as strictly worse.
- **Concurrent matches.** `host_port = base + slot` assumes one match at a time.
  Concurrency needs a real port allocator on the provider.
- **Plain-HTTP registry.** Local dev uses `http://registry:5001` and
  `registry_insecure`, so the deploy JWT crosses the wire in cleartext. Fine on a
  laptop; a real consideration if that config ever escapes.

## Verification log

(Phase 0 results go here: msb version, kernel version, and each assertion's
outcome. Follow the format used in `gvisor-migration.md`.)
