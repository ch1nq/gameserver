#!/usr/bin/env bash
# Set up a Linux host to run game matches on Docker + gVisor (runsc).
#
# This replaces the firecracker-containerd backend. It needs NO KVM, no
# from-source builds, no guest kernel/rootfs, and no devmapper pool: just Docker,
# the runsc runtime (installed from gVisor's release repo), a dedicated Docker
# address pool, and two static iptables INPUT rules that stop guests from
# reaching host services.
#
# Idempotent and re-run safe. Reverse with ./uninstall.sh.
set -euo pipefail

# Dedicated address pool for per-match networks. The coordinator creates one
# /24 internal network per match slot from this /16; the INPUT rules below key
# off it. Must not overlap the host LAN or Docker's default 172.17.0.0/16.
MATCH_POOL="${MATCH_POOL:-10.210.0.0/16}"
MATCH_POOL_SIZE="${MATCH_POOL_SIZE:-24}"
DAEMON_JSON="/etc/docker/daemon.json"

log() { echo "[setup-gvisor] $*"; }

require_root() {
    if [[ "$EUID" -ne 0 ]]; then
        echo "error: run as root (sudo ./install.sh)"
        exit 1
    fi
}
require_root

# ── dependencies ──────────────────────────────────────────────────────────────
log "Installing dependencies (docker, jq, iptables-persistent)..."
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
# iptables-persistent prompts unless preseeded; keep it quiet.
echo "iptables-persistent iptables-persistent/autosave_v4 boolean true" | debconf-set-selections
echo "iptables-persistent iptables-persistent/autosave_v6 boolean true" | debconf-set-selections
apt-get install -y -qq jq iptables-persistent

if ! command -v docker >/dev/null 2>&1; then
    log "Installing Docker..."
    curl -fsSL https://get.docker.com | sh
fi
systemctl enable --now docker >/dev/null 2>&1 || true

# ── runsc (gVisor), from the official release repo ────────────────────────────
# Versioned, checksum-verified binaries — no build step, upgraded via apt.
if ! command -v runsc >/dev/null 2>&1; then
    log "Installing runsc (gVisor) from the official APT repo..."
    curl -fsSL https://gvisor.dev/archive.key | gpg --dearmor -o /usr/share/keyrings/gvisor-archive-keyring.gpg
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/gvisor-archive-keyring.gpg] https://storage.googleapis.com/gvisor/releases release main" \
        > /etc/apt/sources.list.d/gvisor.list
    apt-get update -qq
    apt-get install -y -qq runsc
fi
log "runsc: $(runsc --version | head -1)"

# ── register runsc + match address pool in daemon.json ────────────────────────
# Merge into any existing config (this host may already set bip/dns/log-opts).
log "Configuring ${DAEMON_JSON} (runsc runtime + address pool ${MATCH_POOL})..."
mkdir -p /etc/docker
[[ -f "$DAEMON_JSON" ]] || echo '{}' > "$DAEMON_JSON"

tmp="$(mktemp)"
jq \
    --arg base "$MATCH_POOL" \
    --argjson size "$MATCH_POOL_SIZE" \
    '
    .runtimes["runsc"] = {"path": "/usr/bin/runsc"}
    | (.["default-address-pools"] //= [])
    | if any(.["default-address-pools"][]; .base == $base) then .
      else .["default-address-pools"] += [{"base": $base, "size": $size}] end
    ' "$DAEMON_JSON" > "$tmp"
mv "$tmp" "$DAEMON_JSON"
log "  daemon.json now:"; jq . "$DAEMON_JSON" | sed 's/^/    /'

# runsc install also writes the runtime entry, but doing it via jq above keeps
# the merge explicit and idempotent. Restart to apply address pools + runtime.
log "Restarting Docker to apply configuration..."
systemctl restart docker

# ── static guest->host firewall policy ────────────────────────────────────────
# Match networks are internal (no NAT, no internet) and topology already blocks
# agent<->agent. The one thing topology does NOT cover is a guest connecting to
# services on the host itself (sshd, postgres, the registry, the coordinator).
# Two rules, installed ONCE (not per match): allow established replies back to
# host-initiated connections (coordinator -> game host gRPC), drop everything
# else originating from the match pool.
#
# Inserted into DOCKER-USER so they survive Docker's own chain management and
# are evaluated before Docker's ACCEPTs. Idempotent: delete-then-insert.
log "Installing static guest->host INPUT rules for ${MATCH_POOL}..."

ensure_rule() {
    # $1 = chain, rest = rule spec
    local chain="$1"; shift
    iptables -D "$chain" "$@" 2>/dev/null || true
    iptables -I "$chain" "$@"
}

# On INPUT (traffic addressed to the host itself):
#   1. accept replies to host-initiated flows
#   2. drop everything else from the match pool
# Insert order matters: the ACCEPT must sit above the DROP, so insert DROP first.
ensure_rule INPUT -s "$MATCH_POOL" -j DROP
ensure_rule INPUT -s "$MATCH_POOL" -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT

log "Persisting iptables rules..."
netfilter-persistent save >/dev/null 2>&1 || iptables-save > /etc/iptables/rules.v4

log "Done. Verify with:"
log "  docker info | grep -i runtimes            # runsc listed"
log "  docker run --rm --runtime=runsc alpine uname -a   # 'gVisor' in the kernel string"
log "  sudo iptables -S INPUT | grep ${MATCH_POOL}"
log ""
log "Then set MACHINE_PROVIDER=gvisor in the website .env."
