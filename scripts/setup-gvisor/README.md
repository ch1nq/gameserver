# gVisor Host Setup

Sets up a Linux host to run game matches as Docker containers under the
**gVisor runtime (`runsc`)**, with isolation derived from network topology plus
a small static firewall policy. This is the production backend
(`MACHINE_PROVIDER=gvisor`); it replaces the firecracker-containerd setup in
`scripts/setup-firecracker/`.

> **No KVM required.** gVisor's default (systrap) platform runs in userspace, so
> this works on any Linux box, including VMs that can't nest virtualization and
> local dev machines. (You *can* opt into `--platform=kvm` later for defense in
> depth where `/dev/kvm` exists.)

## What gets installed

| Component | How |
|-----------|-----|
| Docker | `get.docker.com` (only if missing) |
| `runsc` (gVisor) | gVisor's official APT repo — **versioned release binaries**, `apt upgrade`able |
| `runsc` runtime entry + match address pool | merged into `/etc/docker/daemon.json` (existing keys preserved) |
| Two static guest→host `INPUT` rules | `iptables`, persisted via `iptables-persistent` |

No Go toolchain, no from-source builds, no guest kernel/rootfs, no devmapper
thin-pool, no `br_netfilter`, no jailer user, no ttrpc socket.

## Quick start

```bash
sudo ./install.sh
# override the match pool if 10.210.0.0/16 conflicts with your LAN:
#   MATCH_POOL=10.211.0.0/16 sudo ./install.sh

# verify
docker info | grep -iA2 runtimes                       # runsc listed
docker run --rm --runtime=runsc alpine dmesg | head -1 # "Starting gVisor..."
sudo iptables -S INPUT | grep 10.210                   # the two rules
```

Then set in the website `.env`:

```bash
MACHINE_PROVIDER=gvisor
GVISOR_RUNTIME=runsc          # use "runc" only to test topology without gVisor
MACHINE_CPUS=1
MACHINE_MEM_MIB=512
MACHINE_PIDS_LIMIT=256
DOCKER_REGISTRY_PULL_HOST=localhost:5001
```

## Networking / isolation model

Each match gets one **internal** Docker network per slot, carved from the
dedicated pool (`default-address-pools`, default `10.210.0.0/16`, /24 each). The
coordinator (in the website process, on the host) drives the Docker daemon
directly:

```
Host (website + coordinator + dockerd + runsc)
├── net achtung-{match}-s0 (internal)   ← coordinator dials the game host here
│     └── game-host container (slot 0)
├── net achtung-{match}-s1 (internal)
│     ├── game-host container (also attached, before start)
│     └── agent 1 container
└── net achtung-{match}-s2 (internal)
      ├── game-host container (also attached, before start)
      └── agent 2 container
```

| Threat | Control |
|---|---|
| agent ↔ agent | No shared L2 segment — agents are on different networks; cross-network traffic is dropped by Docker's own isolation chains. Fails **closed**, no `br_netfilter`. |
| agent → internet | `internal: true` networks: no NAT, no default route, no DNS. |
| agent → host services (sshd, postgres, registry, coordinator) | The two static `INPUT` rules: drop everything from the match pool, except replies to host-initiated flows (coordinator → game host gRPC). |
| kernel attack surface | `runsc`: guest syscalls hit gVisor's userspace kernel, not the host kernel directly. |
| fork/mem/CPU/disk abuse | Per-container cgroup limits (`MACHINE_CPUS`/`MACHINE_MEM_MIB`/`MACHINE_PIDS_LIMIT`); agents get a read-only rootfs + sized tmpfs. |

**Why the game host is multi-homed and started last:** gVisor cannot attach a
network to a *running* sandbox, so the coordinator connects the game-host
container to every agent network *before* starting it. This is why
`init_match` needs the slot count up front.

## Validating isolation (hostile-agent probe)

`hostile-agent/` builds an image that actively tries to break out — internet
egress, DNS, reaching peer agents, connecting to host services, a fork bomb —
and reports PASS for every attempt that was correctly **blocked**. Keep it as a
permanent smoke test; the old backend had no equivalent.

```bash
docker build -t achtung-hostile-agent scripts/setup-gvisor/hostile-agent
# run it on a match agent network with a peer IP to probe:
docker run --rm --runtime=runsc --network <a match slot net> \
  -e PEER_IPS="10.210.1.3" -e GAME_HOST_IP="10.210.1.2" \
  achtung-hostile-agent
```

The Rust example `cargo run -p achtung-agent-infra --example gvisor_smoke`
(optionally `GVISOR_SMOKE_RUNTIME=runsc`) exercises the same matrix through the
real provider code.

## Uninstall

```bash
sudo ./uninstall.sh            # remove daemon.json entries + INPUT rules + stray
                               # achtung-* networks/containers; KEEP runsc
sudo ./uninstall.sh --purge    # also apt-remove runsc and its repo
```

## Troubleshooting

**`runsc` not listed in `docker info`:** ensure `/etc/docker/daemon.json` has a
`runtimes.runsc` entry and restart Docker (`systemctl restart docker`).

**Match containers fail to start under runsc:** check
`journalctl -u docker` and `/var/log/runsc/`. Some images relying on exotic
syscalls may need `--platform=kvm`; the default systrap platform covers normal
gRPC workloads.

**Host can't reach the game host:** confirm the match networks come from the
pool (`docker network inspect achtung-<id>-s0`) and that the two `INPUT` rules
allow `RELATED,ESTABLISHED` — the coordinator initiates the connection, so
replies must be allowed back.

**Agents can reach each other:** they should be on different networks. Verify
with `docker network inspect` that each agent container is attached to exactly
one `achtung-<id>-s{n}` network and only the game host spans multiple.
