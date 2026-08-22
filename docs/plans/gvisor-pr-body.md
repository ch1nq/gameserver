## Replace the Firecracker backend with gVisor on Docker

Migrates the production match backend from Firecracker/firecracker-containerd to
Docker + the gVisor runtime (`runsc`), trading a large from-source maintenance
surface for a ~40-line host setup while keeping VM-adjacent isolation. Full
rationale, design decisions, and phase checklist in
`docs/plans/gvisor-migration.md`.

### What changes

- **Provider (`libs/agent-infra`)**: `MachineProvider::init_match` now takes a
  slot count. `DockerMachineProvider` gains a `DockerIsolation` enum —
  `SharedNetwork` (local dev, unchanged) and `PerMatchNetworks` (production):
  one **internal** Docker network per match slot, the game host multi-homed
  across all of them *before start* (gVisor can't attach networks to a running
  sandbox), IP-based addressing, `runsc` runtime, per-container cgroup limits,
  and read-only rootfs + sized tmpfs for agents. The reaper now also sweeps
  orphaned `achtung-*` networks (`OrphanedResource` gained a `kind`).
- **Isolation by topology**, not firewall rules: agents live on separate
  networks so agent↔agent fails *closed* (no `br_netfilter` dependency);
  `internal` networks block egress; two static guest→host `INPUT` rules cover
  the host-services case.
- **Wiring**: `MACHINE_PROVIDER=gvisor` (new default) in the website; coordinator
  passes `agents_per_game + 1`. `.env.example` refreshed.
- **Host setup (`scripts/setup-gvisor/`)**: install/uninstall scripts (runsc from
  the official APT repo, jq-merged `daemon.json`, persisted INPUT rules) + a
  hostile-agent probe image that asserts the isolation matrix.
- **Removed**: the entire Firecracker backend, `scripts/setup-firecracker/`,
  vendored protos + vendor script, and the now-unused
  containerd-client/ttrpc/prost-types/ipnet deps.

### Validation

- Phase 0 (Docker 29.6.2): host↔internal-network reachable; egress + cross-network
  blocked.
- `cargo build`/`test` green across the workspace.
- `cargo run -p achtung-agent-infra --example gvisor_smoke` (runtime `runc`):
  full isolation matrix PASS through the real provider.

### Still to do (needs root / real host — tracked in the plan)

- Re-run the smoke example with `GVISOR_SMOKE_RUNTIME=runsc` once runsc is
  installed locally.
- Full local match with `MACHINE_PROVIDER=gvisor`.
- Phase 6 cutover on the Hetzner box, then retire firecracker-containerd.

> Targets `grpc` (this branch's base). Retarget to `main` once `grpc` merges.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
