#!/usr/bin/env bash
# Reverse everything install.sh + setup-devmapper.sh + a coordinator run can
# leave on the host. Best-effort and idempotent: safe to run repeatedly, and one
# missing item never aborts the rest (hence `set -uo pipefail`, NOT `-e`).
#
#   sudo ./uninstall.sh            # remove configs, service, thin-pool, stray
#                                  # match networking; KEEP binaries + guest kernel
#   sudo ./uninstall.sh --purge    # also remove binaries, CNI, kernel, runtime dir
#
# Deliberately does NOT unload kernel modules or flip live sysctls (ip_forward,
# bridge-nf-call-iptables) — other software may depend on them. The persisted
# drop-ins are removed so a reboot returns to the prior state.
set -uo pipefail

PURGE=0
[[ "${1:-}" == "--purge" ]] && PURGE=1

SOCKET="/run/firecracker-containerd/containerd.sock"
NAMESPACE="achtung"
POOL_NAME="fc-dev-thinpool"
DEVMAPPER_DIR="/var/lib/firecracker-containerd/snapshotter/devmapper"
DATA_FILE="${DEVMAPPER_DIR}/data"
META_FILE="${DEVMAPPER_DIR}/metadata"
RUNTIME_DIR="/var/lib/firecracker-containerd/runtime"

log()  { echo "[uninstall] $*"; }
kept() { echo "[uninstall]   KEPT: $*"; }

require_root() {
    if [[ "$EUID" -ne 0 ]]; then
        echo "error: run as root (sudo ./uninstall.sh [--purge])"
        exit 1
    fi
}

require_root

# ── stop the daemon ───────────────────────────────────────────────────────────
log "Stopping firecracker-containerd service..."
systemctl disable --now firecracker-containerd 2>/dev/null || true
if [[ -f /etc/systemd/system/firecracker-containerd.service ]]; then
    rm -f /etc/systemd/system/firecracker-containerd.service
    systemctl daemon-reload
fi

# ── best-effort kill of stray microVMs / tasks ────────────────────────────────
# Only meaningful if the daemon happens to still be up (e.g. started manually).
if [[ -S "$SOCKET" ]] && command -v ctr >/dev/null 2>&1; then
    log "Cleaning up leftover containers/tasks in namespace ${NAMESPACE}..."
    ctr_ns() { ctr --address "$SOCKET" --namespace "$NAMESPACE" "$@" 2>/dev/null; }
    while read -r cid _; do
        [[ -z "$cid" || "$cid" == "CONTAINER" ]] && continue
        ctr_ns task kill  "$cid" || true
        ctr_ns task delete "$cid" || true
        ctr_ns container delete "$cid" || true
    done < <(ctr_ns containers list || true)
fi

# ── flush stray per-match networking ──────────────────────────────────────────
# On a clean teardown the coordinator removes these itself; a crash leaves them.
log "Removing stray match bridges (br-m-*) and their FORWARD rules..."
# Delete leftover FORWARD rules referencing our bridges first (they reference the
# bridge by name, so drop them before deleting the interface).
if command -v iptables-save >/dev/null 2>&1; then
    while read -r rule; do
        [[ -z "$rule" ]] && continue
        # rule looks like: -A FORWARD -o br-m-xxxx -j DROP  → turn -A into -D
        # shellcheck disable=SC2086
        iptables ${rule/-A/-D} 2>/dev/null || true
    done < <(iptables-save 2>/dev/null | grep -- '-A FORWARD' | grep 'br-m-')
fi
# Delete the bridges themselves.
for br in $(ip -o link show type bridge 2>/dev/null | awk -F': ' '{print $2}' | grep '^br-m-'); do
    ip link delete "$br" 2>/dev/null && log "  deleted bridge ${br}" || true
done
# Any orphaned TAPs (bridge already gone) share the tap-m- prefix.
for tap in $(ip -o link show 2>/dev/null | awk -F': ' '{print $2}' | grep '^tap-m-'); do
    ip link delete "$tap" 2>/dev/null && log "  deleted tap ${tap}" || true
done

# ── devmapper thin-pool ───────────────────────────────────────────────────────
log "Removing devmapper thin-pool ${POOL_NAME}..."
if command -v dmsetup >/dev/null 2>&1 && dmsetup status "$POOL_NAME" &>/dev/null; then
    dmsetup remove "$POOL_NAME" 2>/dev/null || log "  (thin-pool busy; may need a reboot to release)"
fi
# Detach loop devices backing the pool files.
for f in "$DATA_FILE" "$META_FILE"; do
    for loop in $(losetup -j "$f" 2>/dev/null | cut -d: -f1); do
        losetup -d "$loop" 2>/dev/null && log "  detached ${loop} ($f)" || true
    done
done
if [[ -d "$DEVMAPPER_DIR" ]]; then
    rm -rf "$DEVMAPPER_DIR"
    log "  removed ${DEVMAPPER_DIR}"
fi

# ── persisted network tunables (drop-ins only; live values left alone) ────────
log "Removing persisted sysctl / module drop-ins (live values left as-is)..."
rm -f /etc/sysctl.d/99-firecracker-bridge.conf
rm -f /etc/modules-load.d/br_netfilter.conf

# ── daemon config ─────────────────────────────────────────────────────────────
log "Removing daemon config..."
rm -rf /etc/firecracker-containerd
rm -f  /etc/containerd/firecracker-runtime.json
rmdir --ignore-fail-on-non-empty /etc/containerd 2>/dev/null || true
# Daemon root/state (safe: dedicated to firecracker-containerd).
rm -rf /var/lib/firecracker-containerd/containerd
rm -rf /run/firecracker-containerd

# ── optional: purge binaries + kernel ─────────────────────────────────────────
if [[ "$PURGE" -eq 1 ]]; then
    log "--purge: removing binaries, guest kernel, runtime dir, and build source..."
    rm -f /usr/local/bin/firecracker \
          /usr/local/bin/firecracker-containerd \
          /usr/local/bin/containerd-shim-aws-firecracker \
          /usr/local/bin/firecracker-ctr \
          /usr/local/bin/ctr \
          /usr/local/bin/containerd \
          /usr/local/bin/containerd-shim \
          /usr/local/bin/containerd-shim-runc-v1 \
          /usr/local/bin/containerd-shim-runc-v2 \
          /usr/local/bin/containerd-stress
    rm -rf /opt/cni/bin
    rm -rf /opt/firecracker-containerd-src   # install.sh's build checkout
    rm -rf "$RUNTIME_DIR"
    rm -rf /var/lib/firecracker-containerd   # whatever is left
else
    kept "/usr/local/bin binaries (firecracker, firecracker-containerd, ctr, shim) — re-run with --purge to remove"
    kept "guest kernel + agent rootfs under ${RUNTIME_DIR} (slow to rebuild)"
    kept "firecracker-containerd build source at /opt/firecracker-containerd-src"
fi

# ── summary ───────────────────────────────────────────────────────────────────
echo ""
log "Cleanup complete."
log "NOTE: the kvm_amd / br_netfilter modules stay loaded and ip_forward /"
log "      bridge-nf-call-iptables keep their current live values until reboot."
log "      To revert them now without rebooting:"
log "        sysctl net.ipv4.ip_forward=0"
log "        sysctl net.bridge.bridge-nf-call-iptables=0   # only if nothing else needs it"
