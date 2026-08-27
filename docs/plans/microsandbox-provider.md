# Plan: microsandbox `MachineProvider`

Status: **implemented, unvalidated against a real match.** Code for every phase is
landed and unit-tested; none of it has been exercised against a booted microVM.
Phase 0 was folded into implementation rather than run as a separate spike — the
SDK reference settled most of it statically, and the rest is settled by running a
real match. See "Deviations from this plan" and "Validation status" at the bottom
for what is confirmed versus still open.

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

### Phase 0 — verification (superseded)

Run as a throwaway spike was **dropped**. Items 1, 5 and 6 were answerable from
the SDK source and the host; items 2, 3 and 4 are answerable only by booting a
real VM, which a real match does better than a synthetic probe. Outcomes:

- [x] 1. `/dev/kvm` present (`crw-rw----+ root:kvm`), `~/.microsandbox/{bin,lib}`
      populated, `msb 0.6.15`. `setup::is_installed()` / `install()` confirmed
      present in the SDK and wired into `app.rs`.
- [ ] 2. **Private pull** — still open. Whether msb's `microsandbox-image` client
      follows the `WWW-Authenticate` realm to exchange Basic for a bearer token
      is undocumented, and the code path is only reachable with a real agent
      image. Phase 2 is implemented on the assumption that it does; if it does
      not, the fallback is `msb load` and Phase 2 becomes dead code (it is
      additive and harmless either way).
- [x] 3. **Workload actually runs** — confirmed *by construction*: the docs state
      "Sandbox creation is boot-only: configuring an image, ENTRYPOINT, or CMD
      does not execute that workload", and `exec_default_stream` resolves the
      effective ENTRYPOINT + CMD. `apps/achtung-host/Dockerfile` sets an
      ENTRYPOINT, so `NoDefaultCommand` cannot fire for the game host. Whether
      the listener is *reachable* is item 4.
- [ ] 4. **Relay topology** — still open, and still the design's kill switch.
      Settled by the first real match rather than a probe. Mitigated rather than
      verified: `MSB_HOST_BIND` switches the agent relay from `127.0.0.1` to
      `0.0.0.0` without a code change, in case a guest cannot reach a
      loopback-bound published port.
- [x] 5. Published ports need `default_ingress: Allow` — confirmed in the SDK.
      `NetworkPolicyBuilder::default_deny()` sets *both* directions, and
      `from_profiles` documents "Ingress defaults to allow, preserving
      published-port behavior". `policy_for_slot` sets the two defaults
      separately; a unit test asserts it.
- [x] 6. Numeric gateway IP instead of DNS — **not viable, and unnecessary.**
      `Rule::allow_dns()`'s doc explains why: under deny-by-default a DNS query
      has no resolved IP yet, so only the `Group::Host` destination is honored at
      DNS-decision time. Slot 0 therefore gets `allow_dns()` (gateway-scoped
      UDP/TCP 53) and nothing wider. Agents get no DNS at all.

Two ways item 4 still kills the design: if a guest cannot reach a host
`127.0.0.1` listener through `host.microsandbox.internal` there is no relay (see
`MSB_HOST_BIND`); if an egress-denied sandbox *can* reach it anyway, agent
isolation is gone with no second lever available.

### Phase 1 — trait + coordinator

- [x] `SpawnConfig` gains `grpc_port: u16` — the port the process listens on
      *inside* the machine. Docker ignores it; microsandbox needs it for
      `port(host, guest)`. Made a **required** `new()` argument rather than a
      defaulted builder method: a wrong value surfaces as a connection timeout
      minutes later, far from its cause.
- [x] `MachineHandle` gains `grpc_port: Option<u16>`. Docker sets `None`;
      microsandbox sets `Some(base + slot)`.
- [x] Coordinator passes the ports into `SpawnConfig`, and at the two dial sites
      uses `handle.grpc_port.unwrap_or(config.game_host_grpc_port)` /
      `.unwrap_or(config.agent_grpc_port)`.
- [x] Doc comment on `private_ip` per decision 6.
- [x] Update `docker.rs` and `examples/gvisor_smoke.rs` for the new fields.
- [x] `cargo check` + existing tests pass.
- [x] **Added, not in the original plan:** replaced the coordinator's flat
      `sleep(5s)`-then-dial with a retry loop bounded by
      `game_host_connect_timeout` (`GAME_HOST_CONNECT_TIMEOUT_SECS`, default
      60s, 500ms interval). A microVM boot plus a cold image pull is not a
      container start, and a single guessed sleep is either too short (spurious
      failures) or wasted on every match. The game host already does this when
      dialing agents (`achtung_grpc.rs`: 30 attempts, 1s apart), so agent boot
      latency was already absorbed — the coordinator's own dial was the only
      unprotected hop.

### Phase 2 — registry system principal

- [x] `RegistryPrincipal { System, User(UserId) }` as `RegistryAuth::UserId`.
- [x] `parse_user_id`: `"system"` -> `System`; `user-N` -> `User(N)`; else `None`.
- [x] `is_valid_token`: `System` -> verify RS256 signature, `aud`, `iss`, `exp`;
      `User` -> existing bcrypt path, untouched.
- [x] `user_has_access`: `System` -> `false` (decision 5); `User` -> existing
      namespace check, untouched.
- [x] Additive `passthrough` trait method with a `None` default, so
      `TestRegistryAuth` needs no change. `RegistryTokenManager` returns the
      verified JWT for `System`. Returns `RegistryJwtToken` rather than a bare
      string, so `expires_in` can be derived from the token's real `exp`.
- [x] `RegistryAuthConfig` caches the public key PEM at construction, next to
      `signing_key` (`key_id_from_pem` already derives it).
- [x] `token_handler` consults `passthrough` after `is_valid_token`, before scope
      parsing; `expires_in` from the JWT's own `exp`.
- [x] Tests: valid deploy JWT passes through with scope intact; expired rejected;
      foreign-signed rejected; wrong-`aud` rejected; `System` mints nothing;
      user path unregressed.
- [x] **Unplanned, and a live landmine:** `jsonwebtoken` 10 selects its crypto
      backend from crate features and installs `panic!` stubs when it cannot
      decide. Adding the microsandbox dependency pulls in `oci-client`, which
      enables `jsonwebtoken/aws_lc_rs` while this crate enables `rust_crypto` —
      feature unification then enables both, and *every* mint and verify aborts
      at runtime. `ensure_crypto_provider()` installs `rust_crypto` explicitly
      before any encode/decode. Caught only because the new verification tests
      exercise the path; the pre-existing code had no test that signed a token,
      so this would otherwise have surfaced as a production panic on the first
      registry pull.

No new env vars, no compose changes.

### Phase 3 — the provider

New `libs/agent-infra/src/microsandbox.rs`, re-exported from `lib.rs`.
Deliberately mirrors `docker.rs` structure so the two read the same way.

Named `MicrosandboxMachineProviderConfig` to match `DockerMachineProviderConfig`,
and with one field added beyond the plan (`host_bind`):

```rust
pub struct MicrosandboxMachineProviderConfig {
    pub cpus: u8,
    pub memory_mib: u32,
    pub host_port_base: u16,
    pub host_bind: IpAddr,          // added; see below
    pub registry_pull_host: String,
    pub registry_insecure: bool,
    pub max_duration_secs: Option<u64>,
}
```

- [x] `MatchContext { match_id, num_slots }`. `num_slots` added: slot 0's egress
      policy needs the full relay port range, which is only known up front.
      `init_match` records both and runs the pre-flight sweep (decision 4).
- [x] `spawn`: as planned, except the `achtung.created_at` label is unnecessary —
      `SandboxHandle::created_at()` already reports it, so only
      `achtung.managed` / `achtung.match` are set (see `list_orphaned` below).
- [x] `spawn` then calls `exec_default_stream` **without blocking** (decision 2).
      The handle is *drained into `tracing`* by a spawned task rather than
      dropped: whether dropping signals the guest process is undocumented, and
      draining surfaces guest stdout/stderr — the only window into a workload
      that dies during startup. `NoDefaultCommand` maps to `MachineCreation`
      with a message naming the image.
- [x] `policy_for_slot`, with one **correction to the plan**: `default_deny()`
      sets *both* directions to `Deny`, which silently kills the published port.
      Egress and ingress are set separately —
      `default_egress(Deny) + default_ingress(Allow)` — for every slot. Slot 0
      additionally gets `Rule::allow_dns()` plus
      `egress(|e| e.tcp().port_range(base+1, base+n).allow_host())`. Slots 1+ get
      zero rules.
- [x] `destroy`: `Sandbox::get(name)` -> `stop()` -> `remove()`, tolerating
      `SandboxNotFound` (the `is_not_found` analogue) and also
      `SandboxNotRunning`, since a crashed sandbox still needs removing.
- [x] `cleanup_match`: no shared resources; log and return `Ok`.
- [x] `list_orphaned`, **reworked**: the planned
      `list_with(|l| l.label("achtung.match", ..))` cannot work.
      `SandboxListBuilder::label` matches an exact key/value pair and the reaper
      has no match id to supply, while `SandboxHandle` exposes **no label
      accessor** and `SandboxConfig` carries no labels field — so a listed
      sandbox's match id cannot be read back either. Fixed with a constant
      marker label `achtung.managed = "1"` to filter on, then narrowing in Rust
      on `name().starts_with(prefix)` and `created_at()`. Paginates on
      `next_cursor`. Only ever `OrphanKind::Machine`.
- [x] `destroy_orphaned`: stop + remove by name (shares `destroy`'s helper).
- [x] Errors map onto existing `MachineError` variants: image -> `ImageCopy`,
      create -> `MachineCreation`, stop/remove -> `Destruction`.
- [x] Dependency: `microsandbox = { version = "0.6.15", default-features = false,
      features = ["net", "prebuilt"] }`. Builds clean. Two surprises: it links
      against **libcap-ng** (so the website image needs `libcap-ng-dev` to build
      and `libcap-ng0` to run), and `PullPolicy` is re-exported from
      `microsandbox::sandbox`, not the crate root.

**Added beyond the plan — `host_bind` (`MSB_HOST_BIND`, default `127.0.0.1`).**
`.port()` binds loopback, and whether msb's netstack forwards a *guest*
connection to a loopback-bound published port is undocumented — Phase 0 item 4,
the one open question that can sink the relay. Rather than hardcode a guess, the
agent relay bind is configurable, so a failure is one env var away from `0.0.0.0`
instead of a code change. Slot 0 always binds loopback: its consumer is the
coordinator, a host process, so widening it would add exposure for no gain.

`achtung.` label keys are safe; microsandbox reserves only `sandbox.`,
`microsandbox.` and `service.`. Note it also **imports the image's own OCI
labels** at create time, ours winning on collision — so a hostile image setting
`achtung.managed=1` can only make itself more reapable, not less.

### Phase 4 — wiring

- [x] `app.rs`: `"microsandbox"` arm plus `microsandbox_config_from_env()`,
      reusing `MACHINE_CPUS` / `MACHINE_MEM_MIB` and adding
      `MSB_HOST_PORT_BASE`, `MSB_HOST_BIND`, `MSB_REGISTRY_INSECURE`,
      `MSB_MAX_DURATION_SECS`. Reaper prefix `"achtung-"`, same as the other two
      backends.
- [x] Guard provider construction on `is_installed()` so a missing runtime fails
      at startup rather than mid-match. Wrapped as
      `agent_infra::ensure_runtime_installed()`, which also *installs* when
      absent (idempotent), so callers need no dependency on the `microsandbox`
      crate.
- [x] `.env.example` section documenting the `/dev/kvm` requirement, the relay
      topology, and that images come from msb's own cache.
- [x] Compose story — **decided against the plan's recommendation.** The plan
      suggested keeping `MACHINE_PROVIDER=docker` in compose and driving
      microsandbox from a host-run process; instead compose now runs
      `MACHINE_PROVIDER=microsandbox`, so local dev exercises the same backend
      as production rather than leaving it untested by default. This required:
      `devices: [/dev/kvm]`, dropping the `docker.sock` mount, a
      `microsandbox_data` volume for the image cache and sandbox database, and
      `libcap-ng0` in the website runner image.
- [x] **Added:** `DOCKER_REGISTRY_PULL_HOST` changed to `registry:5001` in
      compose. The pull now happens from the website container's own network
      namespace, so `localhost:5001` would resolve to the container itself.
- [x] **Added:** `just load-game-host` recipe. msb keeps its own image cache and
      cannot see the Docker daemon's images, so a locally-built game host has to
      be handed over with `docker save | msb load`. Also `msb-images`,
      `msb-sandboxes`, `msb-clean` for inspecting state while debugging.

### Phase 5 — validation

- [x] Unit tests on the pure parts (8 in `microsandbox.rs`): sandbox/label
      naming, `host_port(slot)`, `policy_for_slot` shape for slot 0 / agents /
      single-slot, image-ref construction for public and private. Plus 4 new JWT
      verification tests in `registry-auth`.
- [~] `microsandbox_smoke.rs` — **dropped deliberately.** It would assert the
      isolation matrix against sandboxes it constructs itself, not against the
      ones the provider builds, so it can pass while the provider is wrong. The
      real match below exercises the same paths for real. If the matrix needs
      machine-checking later, the right shape is a test that drives
      `MicrosandboxMachineProvider` itself and probes via `Sandbox::get` +
      `exec`, gated behind a feature or `#[ignore]` so it does not need KVM in
      CI.
- [ ] Real local match: `just load-game-host`, `docker compose up`, spectator
      stream visible at `localhost:3000`. **This is the remaining gate** — it is
      what settles Phase 0 items 2 and 4.
- [ ] Hostile-agent image from `gvisor-migration.md` Phase 4 re-run against this
      backend.

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

## Deviations from this plan

Summarised in one place; each is argued at its phase above.

1. **`default_deny()` would have broken the relay silently.** It sets *both*
   directions to `Deny`, closing the published port. Egress and ingress are set
   separately instead. A unit test pins this.
2. **`list_orphaned` could not be written as specified.** `SandboxHandle` has no
   label accessor and `list_with(...).label(k, v)` needs a concrete value, so a
   constant `achtung.managed = "1"` marker label was added to filter on, with the
   match-id narrowing done in Rust.
3. **`host_bind` / `MSB_HOST_BIND` added.** Turns the plan's single open risk
   (guest reaching a loopback-bound published port) into a config switch instead
   of a code change.
4. **`achtung.created_at` label dropped.** `SandboxHandle::created_at()` already
   provides it; Docker needs the label only because its API does not.
5. **The `ExecHandle` is drained, not dropped.** Dropping has undocumented
   effects on the guest process, and draining is what surfaces a workload that
   dies on startup.
6. **Coordinator dial retry added** (not in the plan). See Phase 1.
7. **Compose runs microsandbox**, against the plan's recommendation, so the
   production backend is what local dev exercises.
8. **`jsonwebtoken` crypto provider must be installed explicitly.** Adding this
   dependency silently turned every JWT mint into a runtime panic; see Phase 2.
9. **`microsandbox_smoke.rs` dropped.** It would test sandboxes it builds itself
   rather than the provider's, so it could pass while the provider is broken.

## Validation status

Environment: `msb 0.6.15`, kernel `6.14.0-33-generic` x86_64, `/dev/kvm` present
(`crw-rw----+ root:kvm`), runtime already installed at `~/.microsandbox`.

Confirmed statically (SDK source / docs / host):

- KVM and runtime availability; `is_installed()` / `install()` wired in.
- Creation is boot-only, so `exec_default_stream` is mandatory; the game host's
  Dockerfile sets an ENTRYPOINT, so `NoDefaultCommand` cannot fire for slot 0.
- Published ports require ingress `Allow`.
- DNS under deny-by-default must go through `Group::Host`; a numeric gateway IP
  is not a viable substitute.
- `RegistryAuth` has only a `Basic` variant, forcing the `system:<jwt>` shape.
- Clean `cargo build`; 8 provider unit tests and 4 JWT tests pass; whole
  workspace checks and tests clean.

Still unproven — needs one real match:

- **The relay.** Whether the game host inside a guest can reach
  `host.microsandbox.internal:{base+n}` when that port is bound on the host's
  loopback. Fallback: `MSB_HOST_BIND=0.0.0.0`.
- **Agent isolation in practice.** That an egress-denied sandbox genuinely cannot
  reach a sibling's relay port. Asserted structurally (zero rules) and by unit
  test, but not observed.
- **Private agent pulls.** Whether msb's image client follows the
  `WWW-Authenticate` realm, which is what makes the Phase 2 passthrough
  reachable at all.
- **Timing.** Whether a microVM boot plus a cold pull fits in the default 60s
  connect budget.

A build-time note that will bite in CI before any of the above: linking now
requires the **libcap-ng development symlink** (`libcap-ng.so`). Only the
versioned `.so.0` ships in the runtime package, so a host with just `libcap-ng0`
fails at link time with `unable to find library -lcap-ng`. The website Dockerfile
installs `libcap-ng-dev` for the build stage and `libcap-ng0` for the runner; a
bare-metal build needs `libcap-ng-dev` too.
