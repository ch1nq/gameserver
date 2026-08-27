#!/bin/sh
# Hostile-agent isolation probes. Each check PASSES when the malicious action is
# BLOCKED. Env:
#   PEER_IPS      space-separated agent IPs that must be unreachable
#   GAME_HOST_IP  the game host IP that SHOULD be reachable (sanity check)
#   PROBE_PORT    TCP port to test for peer/game-host reachability (default 50052)
set -u
PROBE_PORT="${PROBE_PORT:-50052}"
fail=0

pass() { echo "  PASS  $1"; }
bad()  { echo "  FAIL  $1"; fail=1; }

tcp_open() { nc -w 2 "$1" "$2" </dev/null >/dev/null 2>&1; }

echo "== hostile-agent probes =="

# 1. Internet egress (IP-level, DNS is expectedly gone on internal networks).
if tcp_open 1.1.1.1 443; then bad "internet egress reachable (1.1.1.1:443)"; else pass "internet egress blocked"; fi

# 2. DNS resolution should not work on an internal network.
if nslookup example.com >/dev/null 2>&1; then bad "DNS resolution works"; else pass "DNS blocked"; fi

# 3. Peer agents must be unreachable at every address the attacker might guess.
for ip in ${PEER_IPS:-}; do
    if tcp_open "$ip" "$PROBE_PORT"; then bad "peer $ip:$PROBE_PORT reachable"; else pass "peer $ip blocked"; fi
    if ping -c1 -W2 "$ip" >/dev/null 2>&1; then bad "peer $ip pingable"; else pass "peer $ip not pingable"; fi
done

# 4. Host services via the default gateway (the host end of our bridge).
GW="$(ip route | awk '/default/ {print $3; exit}')"
if [ -z "$GW" ]; then
    # Internal networks have no default route; find the bridge gateway directly.
    GW="$(ip route | awk '/ via / {print $3; exit}')"
    GW="${GW:-$(ip -4 addr show | awk '/inet / {print $2}' | sed 's#\.[0-9]*/.*#.1#' | head -1)}"
fi
echo "  (probing host gateway ${GW:-<none>})"
for port in 22 5432 5000 5001; do
    if [ -n "$GW" ] && tcp_open "$GW" "$port"; then bad "host $GW:$port reachable"; else pass "host $GW:$port blocked"; fi
done

# 5. Fork-bomb containment: spawn past the pids_limit and confirm the cap holds
#    (we survive rather than taking down the host). Best-effort, non-fatal.
echo "  (fork pressure test — capped by pids_limit)"
i=0
while [ "$i" -lt 1000 ]; do sleep 30 & i=$((i+1)); done 2>/dev/null
spawned="$(jobs -p 2>/dev/null | wc -l)"
echo "  INFO  spawned ~$spawned background procs before the cap kicked in"
kill $(jobs -p 2>/dev/null) 2>/dev/null || true

# 6. Sanity: the game host SHOULD be reachable, else the match itself is broken.
if [ -n "${GAME_HOST_IP:-}" ]; then
    if tcp_open "$GAME_HOST_IP" "$PROBE_PORT"; then pass "game host reachable (expected)"; else bad "game host $GAME_HOST_IP unreachable — match would fail"; fi
fi

echo "=========================="
if [ "$fail" -eq 0 ]; then echo "hostile-agent: ALL CHECKS PASS (isolation holds)"; else echo "hostile-agent: FAILURES DETECTED"; fi
echo "(idling for inspection; docker rm -f this container when done)"
# Idle so the container can be inspected; exit code reflects the verdict on kill.
sleep infinity
