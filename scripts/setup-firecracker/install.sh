#!/usr/bin/env bash
# Set up a bare-metal / KVM host to run game matches on firecracker-containerd.
# Tested on Ubuntu 22.04 (x86_64) on a Hetzner dedicated server.
#
# IMPORTANT: firecracker-containerd publishes NO release binaries, so it must be
# BUILT FROM SOURCE (Go + Docker). This script installs the build toolchain,
# builds firecracker-containerd (daemon + shim + snapshotter + a *static* in-VM
# agent), builds the guest kernel + VM agent rootfs, installs the prebuilt
# firecracker binary, writes the daemon config / runtime JSON / systemd unit,
# enables br_netfilter, and sets up the devmapper thin-pool.
#
# Re-run safe-ish, but intended for a fresh host. Reverse with ./uninstall.sh.
set -euo pipefail

FIRECRACKER_VERSION="1.10.1"           # prebuilt release binary (proven working)
GO_VERSION="1.22.12"                    # host Go; fcctd's toolchain directive may fetch newer
FC_CTD_REPO="https://github.com/firecracker-microvm/firecracker-containerd"
FC_CTD_REF="${FC_CTD_REF:-main}"       # pin a commit/tag by exporting FC_CTD_REF
FC_CTD_SRC="/opt/firecracker-containerd-src"

ARCH="x86_64"
INSTALL_DIR="/usr/local/bin"
FCCTD_CONFIG="/etc/firecracker-containerd/config.toml"
FCCTD_RUNTIME_JSON="/etc/containerd/firecracker-runtime.json"
FIRECRACKER_RUNTIME_DIR="/var/lib/firecracker-containerd/runtime"

log() { echo "[setup] $*"; }
here="$(cd "$(dirname "$0")" && pwd)"

require_root() {
    if [[ "$EUID" -ne 0 ]]; then
        echo "error: run as root (sudo ./install.sh)"
        exit 1
    fi
}
require_root

# ── build toolchain (git, make, gcc, docker) ──────────────────────────────────
log "Installing build toolchain (apt)..."
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq \
    git build-essential make curl ca-certificates \
    iptables dmsetup e2fsprogs

# Docker: only install if missing, via the convenience script (avoids the
# docker.io-vs-docker-ce apt conflict on hosts that already have Docker).
if ! command -v docker >/dev/null 2>&1; then
    log "Installing Docker..."
    curl -fsSL https://get.docker.com | sh
fi
systemctl enable --now docker >/dev/null 2>&1 || true

# ── Go (Ubuntu's is too old for firecracker-containerd) ───────────────────────
if ! /usr/local/go/bin/go version 2>/dev/null | grep -q "go${GO_VERSION%.*}"; then
    log "Installing Go ${GO_VERSION}..."
    curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz" -o /tmp/go.tgz
    rm -rf /usr/local/go && tar -C /usr/local -xzf /tmp/go.tgz && rm /tmp/go.tgz
fi
export PATH="$PATH:/usr/local/go/bin"

# ── firecracker (prebuilt release binary) ─────────────────────────────────────
log "Installing firecracker ${FIRECRACKER_VERSION}..."
FC_URL="https://github.com/firecracker-microvm/firecracker/releases/download/v${FIRECRACKER_VERSION}/firecracker-v${FIRECRACKER_VERSION}-${ARCH}.tgz"
tmpdir=$(mktemp -d)
curl -fsSL "$FC_URL" | tar -C "$tmpdir" -xz
install -m 0755 "$tmpdir/release-v${FIRECRACKER_VERSION}-${ARCH}/firecracker-v${FIRECRACKER_VERSION}-${ARCH}" \
    "${INSTALL_DIR}/firecracker"
rm -rf "$tmpdir"

# ── firecracker-containerd (build from source) ────────────────────────────────
log "Cloning + building firecracker-containerd (${FC_CTD_REF})..."
if [[ ! -d "$FC_CTD_SRC/.git" ]]; then
    git clone "$FC_CTD_REPO" "$FC_CTD_SRC"
fi
git -C "$FC_CTD_SRC" fetch --depth 1 origin "$FC_CTD_REF"
git -C "$FC_CTD_SRC" checkout -q FETCH_HEAD || git -C "$FC_CTD_SRC" checkout -q "$FC_CTD_REF"

pushd "$FC_CTD_SRC" >/dev/null
log "  building daemon + shim + snapshotter..."
make all
make install     # installs firecracker-containerd, containerd-shim-aws-firecracker, firecracker-ctr, snapshotter

# The in-VM agent MUST be statically linked: it is placed into an older-glibc
# Debian rootfs, and a dynamically-linked agent (built against the host glibc)
# fails to start ("GLIBC_2.xx not found") → the VM never comes up over vsock.
# `agent-in-docker` builds it with STATIC_AGENT=on (CGO_ENABLED=0).
log "  building STATIC in-VM agent..."
rm -f agent/agent
make -C agent clean || true
make agent-in-docker

log "  building guest kernel + VM agent rootfs (Docker)..."
rm -f tools/image-builder/rootfs.img
rm -rf tools/image-builder/files_ephemeral
make image
make install-default-vmlinux install-default-rootfs
popd >/dev/null

# firecracker-ctr matches the daemon exactly; expose it as `ctr` (the coordinator
# shells out to `ctr`). Avoids a separate containerd download + version-skew.
ln -sf "${INSTALL_DIR}/firecracker-ctr" "${INSTALL_DIR}/ctr"

# ── firecracker-containerd daemon config + runtime + service ──────────────────
log "Writing daemon config to ${FCCTD_CONFIG}..."
mkdir -p "$(dirname "$FCCTD_CONFIG")"
cp "${here}/firecracker-containerd-config.toml" "$FCCTD_CONFIG"

log "Writing runtime config to ${FCCTD_RUNTIME_JSON}..."
mkdir -p "$(dirname "$FCCTD_RUNTIME_JSON")"
cp "${here}/firecracker-runtime.json" "$FCCTD_RUNTIME_JSON"

log "Installing firecracker-containerd systemd unit..."
cp "${here}/firecracker-containerd.service" /etc/systemd/system/firecracker-containerd.service
systemctl daemon-reload

# ── bridge netfilter (REQUIRED for agent isolation) ───────────────────────────
# Agent<->agent DROP rules are iptables FORWARD rules. Intra-bridge traffic only
# traverses FORWARD when br_netfilter is loaded and bridge-nf-call-iptables=1.
# Without this the isolation guarantee silently fails open. ip6tables likewise:
# the coordinator installs IPv6 DROP rules per match bridge, and they only see
# bridged traffic with bridge-nf-call-ip6tables=1.
log "Enabling br_netfilter + bridge-nf-call-ip{,6}tables..."
modprobe br_netfilter || true
echo "br_netfilter" > /etc/modules-load.d/br_netfilter.conf
cat > /etc/sysctl.d/99-firecracker-bridge.conf <<'EOF'
net.bridge.bridge-nf-call-iptables = 1
net.bridge.bridge-nf-call-ip6tables = 1
net.ipv4.ip_forward = 1
EOF
sysctl -p /etc/sysctl.d/99-firecracker-bridge.conf || true

# ── jailer identity ───────────────────────────────────────────────────────────
# The coordinator asks firecracker-containerd to jail each VMM via its runc
# jailer, running the firecracker process as this unprivileged uid/gid
# (FirecrackerMachineProviderConfig::jailer_uid, default 52525). The numeric id
# is what matters; the passwd entry just keeps it visible in ps/audit output.
if ! getent passwd fc-jailer >/dev/null; then
    log "Creating fc-jailer system user (uid/gid 52525)..."
    groupadd --system --gid 52525 fc-jailer
    useradd --system --uid 52525 --gid 52525 --no-create-home \
        --shell /usr/sbin/nologin fc-jailer
fi
# The jailed (non-root) VMM must be able to read the shared kernel image and
# the VM agent rootfs.
chmod 0644 "${FIRECRACKER_RUNTIME_DIR}/default-vmlinux.bin" \
           "${FIRECRACKER_RUNTIME_DIR}/default-rootfs.img" || true

# ── devmapper thin-pool ───────────────────────────────────────────────────────
log "Setting up devmapper thin-pool..."
bash "${here}/setup-devmapper.sh"

# ── verify KVM ────────────────────────────────────────────────────────────────
if [[ ! -e /dev/kvm ]]; then
    echo ""
    echo "WARNING: /dev/kvm not found. Load the KVM module before starting the daemon:"
    echo "  modprobe kvm_intel   # Intel CPUs"
    echo "  modprobe kvm_amd     # AMD CPUs"
    echo "(and ensure virtualization is enabled in BIOS/UEFI)"
fi

log "Installation complete. Next:"
log "  1. systemctl enable --now firecracker-containerd"
log "  2. ctr --address /run/firecracker-containerd/containerd.sock --namespace achtung version"
log ""
log "To reverse all of this later: sudo ./uninstall.sh (add --purge to also remove binaries + source)."
