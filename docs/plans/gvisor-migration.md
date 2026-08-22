# Plan: replace the Firecracker backend with gVisor on Docker

Status: **in progress** — keep the checkboxes below up to date as phases land.
Branch: `gvisor` (based on `grpc`). PR should target `grpc` until `grpc` merges to `main`, then retarget.

## Why

The production match backend (`libs/agent-infra/src/firecracker/`) runs each match
participant in a Firecracker microVM via firecracker-containerd. It works, but the
maintenance surface is disproportionate for a solo maintainer:

- firecracker-containerd ships no release binaries; `scripts/setup-firecracker/install.sh`
  builds it from source off `main` (Go + Docker + guest kernel + rootfs builds), and
  `scripts/vendor-agent-infra-protos.sh` vendors its protos, which can skew from the host build.
- Three control paths to one daemon: containerd gRPC, hand-rolled ttrpc (`fccontrol`), and
  shelling out to `ctr`.
- Hand-rolled network isolation (per-match bridges, TAPs, iptables/ip6tables) that depends on
  `br_netfilter` sysctls and **fails open** if they are unset; crash recovery leaks bridges,
  TAPs, and iptables rules (the reaper only sweeps containers).
- Requires KVM-capable hardware; cannot be tested on machines without KVM.

Replacement: run match containers with plain Docker + the **gVisor runtime (`runsc`)**, and get
isolation from **network topology** instead of firewall rules. gVisor's systrap platform needs no
KVM, so the backend runs on any Linux box (including local dev machines and, in principle, a VM
at any cloud provider).

## Target architecture

The coordinator (in the website process, on the host) drives Docker directly. Per match:

```
net achtung-{match}-s0   (internal)  ← coordinator dials the game host here
      └── game-host container (slot 0)
net achtung-{match}-s1   (internal)
      ├── game-host container (2nd interface)
      └── agent 1 container
net achtung-{match}-s2   (internal)
      ├── game-host container (3rd interface)
      └── agent 2 container
```

Isolation properties and how they are enforced:

| Property | Mechanism |
|---|---|
| agent ↔ agent blocked | No shared L2 segment exists. Cross-bridge traffic is routed, hits `FORWARD`, dropped by Docker's own `DOCKER-ISOLATION` chains. Fails **closed**; no `br_netfilter` needed. |
| No internet egress | All match networks are created with `internal: true`. |
| Guest → host services blocked | Two **static** INPUT rules installed once at host setup (accept `ESTABLISHED,RELATED` from the match-network pool, drop the rest). Not per-match, no dynamic rule churn. |
| Kernel isolation | `runsc` (systrap). Untrusted syscalls terminate in gVisor's userspace kernel. |
| Fork/mem/CPU/disk abuse | cgroup limits via `HostConfig` (`nano_cpus`, `memory`, `pids_limit`) + read-only rootfs with a sized tmpfs `/tmp`. |

## Load-bearing design decisions (do not silently change these)

1. **gVisor cannot attach networks to a running sandbox** (`docker network connect` on a running
   container is unsupported by runsc). Therefore all networks for a match are created in
   `init_match`, and the game-host container is connected to every agent network **between
   `create_container` and `start_container`**. This forces a trait change: `init_match` takes the
   slot count. The coordinator knows `agents_per_game`, so it passes `agents_per_game + 1`.
2. **Address by IP, not container name.** After starting a container, inspect it and put its
   bridge IP in `MachineHandle.private_ip` (game host: its IP on the slot-0 network). The
   coordinator already composes `http://{private_ip}:{port}` — no coordinator changes needed.
   Docker's embedded DNS (127.0.0.11) is deliberately not relied on under runsc.
3. **Docker IPAM owns subnet allocation** via `default-address-pools` in `daemon.json`
   (dedicated pool, e.g. `10.210.0.0/16`, /24 per network). `subnet_pool.rs` is deleted. Docker
   persists networks, so allocation state survives coordinator crashes/restarts.
4. **The old shared-network Docker mode stays** for local dev (website inside a container,
   name-based addressing, default runtime). It becomes one variant of a config enum; the new
   per-match-network mode is the other. Same provider, same `MachineProvider` impl.
5. **Reaper sweeps networks too.** `list_orphaned` must return orphaned `achtung-*` *networks*
   (age from the network's `Created` timestamp) in addition to containers, and
   `destroy_orphaned` must handle both. This fixes an orphan-leak gap the Firecracker backend
   had (bridges/rules leaked on crash).
6. **Validated on Docker 29.x (2026-08): the host CAN reach containers on `internal` networks.**
   If a future Docker version blocks host→internal-bridge traffic, fallback documented in
   Phase 0 notes: non-internal networks + one static `DOCKER-USER` egress drop for the pool range.
   Re-verify on the production host during Phase 6.

## Phases

### Phase 0 — environment verification (this dev machine, then repeat on prod host)
- [x] `docker network create --internal x` + container on it: host can reach container IP; container cannot reach 1.1.1.1 or the host's LAN.
- [x] Two internal networks: containers on different networks cannot reach each other.
- [x] Record Docker version + results in this file under "Verification log".

### Phase 1 — provider changes (`libs/agent-infra`)
- [ ] `lib.rs`: `MachineProvider::init_match(&self, match_id: &str, num_slots: u8)` (breaking
      trait change; update docker, firecracker, and coordinator call sites — firecracker ignores
      the new arg until Phase 5 deletes it).
- [ ] `docker.rs`: add `DockerIsolation` enum to `DockerMachineProviderConfig`:
      `SharedNetwork { network: String }` (existing behavior, unchanged) and
      `PerMatchNetworks { runtime: String, nano_cpus: i64, memory_bytes: i64, pids_limit: i64 }`.
- [ ] `PerMatchNetworks` mode:
      - `init_match`: create `num_slots` networks `achtung-{match}-s{n}`, `internal: true`,
        labels `achtung.match={match_id}`, `achtung.created_at={unix}`. Context carries the list.
      - `spawn` slot 0: create on net s0 → connect to s1..sN → start → inspect → IP of s0.
      - `spawn` slot n≥1: create on net s{n} → start → inspect → IP of s{n}.
      - `HostConfig`: `runtime`, `nano_cpus`, `memory`, `pids_limit`, `readonly_rootfs: true`,
        tmpfs `/tmp` (e.g. 64m). Keep container labels/names identical to today
        (`achtung-{match}-slot-{n}`) so the reaper prefix is unchanged.
      - `cleanup_match`: remove the match networks (containers already destroyed).
- [ ] Reaper: sweep orphaned networks by label/name-prefix + age (decision 5).
- [ ] `cargo check` + existing tests pass; new unit tests where logic is pure.

### Phase 2 — wiring (`apps/website`, `libs/coordinator`)
- [ ] Coordinator passes `agents_per_game + 1` to `init_match`.
- [ ] `app.rs`: `MACHINE_PROVIDER=gvisor` selects Docker provider in `PerMatchNetworks` mode
      with `runtime=runsc` (env overrides: `GVISOR_RUNTIME`, `MACHINE_CPUS`, `MACHINE_MEM_MIB`).
      `MACHINE_PROVIDER=docker` keeps the existing shared-network behavior. `firecracker`
      keeps working until Phase 5.
- [ ] `.env.example` updated.

### Phase 3 — local validation (this machine)
- [ ] Integration example `libs/agent-infra/examples/gvisor_smoke.rs`: init_match(3 slots),
      spawn 3 alpine/netshoot containers, assert: host↔slot0 TCP works; agent→game-host works;
      agent→agent fails; agent→1.1.1.1 fails; agent→host-gateway NEW connection fails once the
      INPUT rules are installed (skip that assert if rules absent, print a warning).
- [ ] Run the smoke example with `runtime=runc` first (topology only), then with `runsc`.
- [ ] Install runsc locally (official release binary → `/usr/local/bin`, `runsc install`,
      restart dockerd). Needs sudo — coordinate with the user.
- [ ] Full local match: `MACHINE_PROVIDER=gvisor` + registry + real agent images
      (see memory: local docker sim / achtung CLI). Spectator stream visible at localhost:3000.

### Phase 4 — host setup script + hostile-agent test
- [ ] `scripts/setup-gvisor/install.sh`: install runsc from gVisor release repo (versioned,
      checksum-verified), write `daemon.json` (runtimes + `default-address-pools`), install the
      two static INPUT rules persistently, restart docker. Idempotent. ~40 lines target.
- [ ] `scripts/setup-gvisor/uninstall.sh`.
- [ ] `scripts/setup-gvisor/README.md`: model description, threat model, troubleshooting.
- [ ] Hostile-agent image (`scripts/setup-gvisor/hostile-agent/`): on start, attempts internet
      egress, agent-subnet scan, host SSH/Postgres connect, fork bomb; reports over its normal
      agent gRPC port so a match with it completes. Keep as a permanent smoke test.

### Phase 5 — remove Firecracker
- [ ] Delete `libs/agent-infra/src/firecracker/`, `subnet_pool.rs` usage, vendored protos
      (`libs/agent-infra/proto/`), `scripts/vendor-agent-infra-protos.sh`,
      `scripts/setup-firecracker/`, ttrpc/containerd-client/prost-build deps from Cargo.toml,
      the `firecracker` arm in `app.rs`, `FIRECRACKER_*` from `.env.example`.
- [ ] `cargo check` clean, no unused deps (`cargo machete` or review).
- [ ] Update memory/docs that call Firecracker the production backend.

### Phase 6 — production cutover (Hetzner box, `ssh hetzner-fc`)
- [ ] Re-run Phase 0 verification on the box.
- [ ] Run `scripts/setup-gvisor/install.sh`; run smoke example + hostile agent there.
- [ ] Run a real match end-to-end with `MACHINE_PROVIDER=gvisor`.
- [ ] Retire firecracker-containerd on the box (`scripts/setup-firecracker/uninstall.sh`)
      after a soak period.

## Verification log

- **2026-08-22, dev machine (Arch, Docker 29.6.2), Phase 0:** two `--internal` networks,
  one alpine container each. host→container ICMP + TCP: OK. container→1.1.1.1 (wget, 3s
  timeout): blocked. cross-network container→container ICMP: blocked. container→its own
  gateway IP: reachable (expected — static INPUT rules are a Phase 4 deliverable).
  Design decision 6 confirmed on this Docker version.
