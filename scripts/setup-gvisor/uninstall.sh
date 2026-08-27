#!/usr/bin/env bash
# Reverse what install.sh + a coordinator run leave on the host. Best-effort and
# idempotent; one missing item never aborts the rest (hence -uo pipefail, not -e).
#
#   sudo ./uninstall.sh            # remove daemon.json entries, INPUT rules,
#                                  # stray match networks/containers; KEEP runsc
#   sudo ./uninstall.sh --purge    # also apt-remove runsc + its repo
set -uo pipefail

PURGE=0
[[ "${1:-}" == "--purge" ]] && PURGE=1

MATCH_POOL="${MATCH_POOL:-10.210.0.0/16}"
DAEMON_JSON="/etc/docker/daemon.json"

log()  { echo "[uninstall-gvisor] $*"; }
kept() { echo "[uninstall-gvisor]   KEPT: $*"; }

require_root() {
    if [[ "$EUID" -ne 0 ]]; then
        echo "error: run as root (sudo ./uninstall.sh [--purge])"
        exit 1
    fi
}
require_root

# ── stray match resources (achtung-*) ─────────────────────────────────────────
log "Removing stray match containers + networks (achtung-*)..."
mapfile -t containers < <(docker ps -aq --filter 'name=achtung-' 2>/dev/null)
[[ ${#containers[@]} -gt 0 ]] && docker rm -f "${containers[@]}" >/dev/null 2>&1 || true
mapfile -t networks < <(docker network ls -q --filter 'label=achtung.match' 2>/dev/null)
[[ ${#networks[@]} -gt 0 ]] && docker network rm "${networks[@]}" >/dev/null 2>&1 || true

# ── daemon.json: drop the runsc runtime + our address pool ─────────────────────
if [[ -f "$DAEMON_JSON" ]] && command -v jq >/dev/null 2>&1; then
    log "Removing runsc runtime + ${MATCH_POOL} pool from ${DAEMON_JSON}..."
    tmp="$(mktemp)"
    jq --arg base "$MATCH_POOL" '
        del(.runtimes.runsc)
        | if (.runtimes | length) == 0 then del(.runtimes) else . end
        | if has("default-address-pools") then
            .["default-address-pools"] |= map(select(.base != $base))
          else . end
        | if (.["default-address-pools"] // []) == [] then del(.["default-address-pools"]) else . end
    ' "$DAEMON_JSON" > "$tmp" && mv "$tmp" "$DAEMON_JSON"
    log "Restarting Docker..."
    systemctl restart docker 2>/dev/null || true
fi

# ── static INPUT rules ────────────────────────────────────────────────────────
log "Removing guest->host INPUT rules for ${MATCH_POOL}..."
iptables -D INPUT -s "$MATCH_POOL" -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || true
iptables -D INPUT -s "$MATCH_POOL" -j DROP 2>/dev/null || true
netfilter-persistent save >/dev/null 2>&1 || iptables-save > /etc/iptables/rules.v4 2>/dev/null || true

# ── binaries ──────────────────────────────────────────────────────────────────
if [[ "$PURGE" -eq 1 ]]; then
    log "Purging runsc..."
    apt-get remove -y -qq runsc 2>/dev/null || true
    rm -f /etc/apt/sources.list.d/gvisor.list /usr/share/keyrings/gvisor-archive-keyring.gpg
else
    kept "runsc binary + apt repo (use --purge to remove)"
fi

log "Done."
