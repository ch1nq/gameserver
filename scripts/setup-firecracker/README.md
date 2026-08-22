# Firecracker Host Setup

This directory contains scripts to set up a bare-metal Linux host (or KVM-capable
VM) for running game matches using firecracker-containerd.

> **Requirement:** The host must support KVM (`/dev/kvm` must be present).
> Firecracker does not work inside Docker or on hosts without KVM.

## What gets installed

> **firecracker-containerd ships no release binaries** — `install.sh` builds it
> **from source** (needs Go + Docker, which the script installs). Expect the
> first run to take ~10–15 min for the build.

| Component | How |
|-----------|-----|
| Build toolchain (git, make, gcc, Docker, Go) | apt + Go tarball |
| `firecracker` | Prebuilt release binary (v1.10.1) |
| `firecracker-containerd` daemon + `aws.firecracker` shim + devmapper snapshotter | **built from source** (`make all install`) |
| `firecracker-ctr` (symlinked to `ctr`) | from the same build — matches the daemon, no version skew |
| Guest kernel (`default-vmlinux.bin`) + **VM agent rootfs** (`default-rootfs.img`) | **built from source** (`make image install-default-*`) |
| firecracker-containerd config + runtime JSON + systemd unit | copied from this dir |

> The in-VM agent is built **statically** (`STATIC_AGENT=on` via `agent-in-docker`).
> A dynamically-linked agent fails to start inside the older-glibc rootfs
> (`GLIBC_2.xx not found`) and the VM never comes up over vsock.

> The pre-created-TAP networking model (below) does **not** use CNI or the
> `tc-redirect-tap` plugin — we create the TAP ourselves and hand it to
> `CreateVM`. The control API is **ttrpc** (on `…/containerd.sock.ttrpc`), not gRPC.

## Quick start

```bash
# 1. Build + install everything (toolchain, firecracker-containerd from source,
#    kernel + rootfs, configs, systemd unit, br_netfilter, devmapper). Slow.
#    Pin a firecracker-containerd commit/tag with FC_CTD_REF=<ref> if desired.
sudo ./install.sh

# 2. Start the dedicated firecracker-containerd daemon
sudo systemctl enable --now firecracker-containerd

# 3. Verify the setup (note the firecracker-containerd socket)
sudo ctr --address /run/firecracker-containerd/containerd.sock \
         --namespace achtung version
```

`setup-devmapper.sh` is invoked by `install.sh`; run it standalone only to
(re)create the thin-pool.

## Environment variables

Set these in your `.env` file (see `.env.example` in the repo root):

```bash
MACHINE_PROVIDER=firecracker
# The dedicated firecracker-containerd daemon socket (serves the control API)
CONTAINERD_SOCKET=/run/firecracker-containerd/containerd.sock
CONTAINERD_NAMESPACE=achtung
FIRECRACKER_KERNEL=/var/lib/firecracker-containerd/runtime/hello-vmlinux.bin
FIRECRACKER_VCPU_COUNT=1
FIRECRACKER_MEM_SIZE_MIB=512
FIRECRACKER_SUBNET_POOL=10.200.0.0/16
```

## Uninstall / cleanup

`uninstall.sh` reverses everything `install.sh` + `setup-devmapper.sh` + a coordinator
run can leave behind. It is best-effort and idempotent (safe to run repeatedly).

```bash
# Stop the service, remove daemon config, tear down the devmapper thin-pool and
# loop devices, delete stray br-m-* bridges + their FORWARD rules, and remove the
# persisted sysctl/module drop-ins. KEEPS binaries and the guest kernel/rootfs.
sudo ./uninstall.sh

# Same, but ALSO remove /usr/local/bin binaries, CNI plugins, the guest kernel,
# and the runtime dir.
sudo ./uninstall.sh --purge
```

It deliberately does **not** unload kernel modules or flip the *live*
`ip_forward` / `bridge-nf-call-iptables` values (other software may rely on
them) — it removes the persisted drop-ins so a reboot returns to the prior
state, and prints the `sysctl` commands to revert them immediately if you want.

## Networking model

Networking is configured per-VM through the firecracker-containerd **CreateVM**
control API using a **pre-created TAP + static IP** (not OCI annotations, not
CNI). Each game match gets its own `/24` subnet and Linux bridge; the coordinator
creates a TAP per machine and passes it to `CreateVM`, which tells the in-VM
agent to statically configure the guest NIC.

```
Host (coordinator)
  └── br-m-{match}  (10.200.X.254/24)
        ├── tap-m-{match}-0  →  game host microVM  (10.200.X.1)
        ├── tap-m-{match}-1  →  agent 1 microVM    (10.200.X.2)
        └── tap-m-{match}-2  →  agent 2 microVM    (10.200.X.3)
```

**Isolation (two layers, because agents are untrusted and control their own
network stack inside the VM):**

L2 — bridge port isolation: every agent TAP is an `isolated` bridge port, so
agents cannot exchange *any* frames with each other (IPv4, IPv6 link-local,
ARP tricks, raw ethertypes), regardless of what addresses a guest assigns
itself. The game-host TAP (slot 0) is non-isolated, so agent↔game-host
traffic flows normally.

L3 — iptables/ip6tables per match bridge:
- FORWARD default-drops the bridge; game host ↔ agents is allowed **within the
  bridge only** (no cross-match, no host-LAN reach)
- INPUT from the bridge is dropped: guests cannot reach host services
  (registry, SSH, coordinator). Replies to host-initiated connections
  (coordinator → game host gRPC) are allowed via conntrack
- All IPv6 on the bridge is dropped (matches are IPv4-only)
- No NAT is configured → microVMs have no internet access

Each VMM process runs jailed as the unprivileged `fc-jailer` user (uid 52525)
via firecracker-containerd's runc jailer, so a VMM escape does not land as
root. Set `FIRECRACKER_JAILER_UID=0` to disable (local testing only).

> **Critical:** the L3 rules only take effect for intra-bridge traffic when
> `br_netfilter` is loaded and `net.bridge.bridge-nf-call-iptables=1` /
> `net.bridge.bridge-nf-call-ip6tables=1`. `install.sh` configures this;
> verify with `sysctl net.bridge.bridge-nf-call-iptables`.

## Agent images

Agent images are arbitrary OCI images submitted by users. They are pulled
directly from the private registry using scoped JWT tokens and run *inside* the
microVM (the image supplies the container rootfs via the devmapper snapshotter;
the VM itself boots the agent rootfs above).

Requirements for agent images:
- Must be a valid Linux OCI image (any base: Alpine, Debian, Ubuntu, etc.)
- Must listen on port `50052` (gRPC Agent service, see `protos/agent.proto`)
- Will receive environment variables set by the coordinator

## Troubleshooting

**`/dev/kvm` not found:**
```bash
modprobe kvm_intel   # or kvm_amd
ls -la /dev/kvm
```

**Daemon fails to start:**
```bash
journalctl -u firecracker-containerd -f
```

**devmapper snapshotter errors:**
```bash
# Check thin-pool status
dmsetup status
# Re-run setup
sudo ./setup-devmapper.sh
```

**microVM fails to boot:**
```bash
journalctl -u firecracker-containerd -f | grep -i firecracker
# Verify kernel + agent rootfs paths match firecracker-runtime.json
ls -la /var/lib/firecracker-containerd/runtime/hello-vmlinux.bin
ls -la /var/lib/firecracker-containerd/runtime/default-rootfs.img
```

**Agents can reach each other (isolation not working):**
```bash
# br_netfilter must be loaded and the sysctl set to 1
lsmod | grep br_netfilter
sysctl net.bridge.bridge-nf-call-iptables   # must be 1
```
