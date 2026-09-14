// Live spectator client. Hand-written (no build step): speaks Server-Sent
// Events to /spectator/watch, which the website relays from the current game
// host's GameHost.WatchGame stream, decoding the protobuf payload to JSON. The
// stream yields `lineup` when a match starts, one `snapshot` event followed by
// per-tick `delta` events during play; we accumulate frames and render the
// Achtung curve to a canvas, and mirror liveness into the "Playing now" list
// (dead bots dim with their live placement, e.g. 8TH — the terminal `result`
// event is intentionally ignored: placements come from death order instead).
//
// The SSE connection stays open across games (with keep-alive comments), so
// there is no manual reconnect/backoff logic here: `waiting` resets to the
// waiting screen, the next `lineup` starts a new game on the same connection.

// Distinct-ish colors per player slot; wraps if there are more players.
// Must match the slot order of the `lineup` event (slot i == player_id i).
const PLAYER_COLORS = [
    "#5fc9ff", "#ff6fa8", "#ffd84a", "#7bf0a8",
    "#b78cff", "#ff9a4d", "#4de0d0", "#f45b5b",
];

function playerColor(id) {
    return PLAYER_COLORS[id % PLAYER_COLORS.length];
}

function init_spectator(canvasId) {
    const canvas = document.getElementById(canvasId);
    const ctx = canvas.getContext("2d");
    const legendEl = document.getElementById("spectator-legend");

    // { arena: {width,height}, players: Map<id, {alive, head, body:[]}> }
    let state = null;
    // slot -> {agent_id, name}
    let lineup = new Map();
    // slot -> live placement ("8TH"), in death order; reset each game.
    let placements = new Map();
    // slot -> {li, bar, name, meta} row elements, rebuilt per game.
    let rows = new Map();

    function drawMessage(text) {
        ctx.fillStyle = "#0A0B10";
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.fillStyle = "#8890b5";
        ctx.font = "20px sans-serif";
        ctx.fillText(text, 20, 36);
    }

    function draw() {
        if (!state) return;
        ctx.fillStyle = "#0A0B10";
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        for (const [id, player] of state.players) {
            ctx.fillStyle = playerColor(id);
            for (const blob of player.body) {
                ctx.beginPath();
                ctx.arc(blob.x, blob.y, blob.size, 0, 2 * Math.PI);
                ctx.fill();
            }
            if (player.alive && player.head) {
                ctx.fillStyle = "#ffffff";
                ctx.beginPath();
                ctx.arc(player.head.x, player.head.y, player.head.size, 0, 2 * Math.PI);
                ctx.fill();
            }
        }
    }

    // "8TH" style placement label, like the mockup status column.
    function ordinal(n) {
        const suffix = n === 1 ? "ST" : n === 2 ? "ND" : n === 3 ? "RD" : "TH";
        return `${n}${suffix}`;
    }

    function aliveCount() {
        if (!state) return lineup.size;
        let n = 0;
        for (const player of state.players.values()) {
            if (player.alive) n++;
        }
        return n;
    }

    // Landing "Playing now" rows: color bar + name + #agent_id,
    // styled like the mockup player tiles (bordered, tabular meta).
    // Builds one row per lineup slot and paints current presence.
    function renderLegend() {
        if (!legendEl) return;
        legendEl.innerHTML = "";
        rows = new Map();
        const slots = [...lineup.entries()].sort((a, b) => a[0] - b[0]);
        for (const [slot, entry] of slots) {
            const li = document.createElement("li");
            const bar = document.createElement("span");
            bar.className = "block flex-none w-4 h-[3px] rounded-[2px]";
            bar.style.backgroundColor = playerColor(slot);
            const label = document.createElement("span");
            label.textContent = entry.name;
            const meta = document.createElement("span");
            meta.textContent = `#${entry.agent_id}`;
            li.appendChild(bar);
            li.appendChild(label);
            li.appendChild(meta);
            legendEl.appendChild(li);
            rows.set(slot, { li, bar, name: label, meta });
        }
        updatePresence();
    }

    // Repaints rows in place (called per frame): alive rows keep the
    // surface tile, dead rows go transparent + muted with their placement.
    function updatePresence() {
        for (const [slot, entry] of lineup) {
            const row = rows.get(slot);
            if (!row) continue;
            const player = state ? state.players.get(slot) : undefined;
            const alive = !player || player.alive;
            const place = placements.get(slot);
            if (!alive) {
                row.li.className = "h-[26px] flex items-center gap-[9px] px-2.5 border border-[var(--line)] rounded bg-transparent";
                row.bar.style.opacity = "0.35";
                row.name.className = "flex-1 min-w-0 font-semibold text-xs text-[var(--muted)] overflow-hidden text-ellipsis whitespace-nowrap";
                row.meta.className = "flex-none text-[11px] font-bold text-[var(--muted)] tabular-nums";
                row.meta.textContent = place === undefined ? "–" : place;
            } else if (place !== undefined) {
                // Sole survivor: surface tile like the mockup winner row,
                // placement in green.
                row.li.className = "h-[26px] flex items-center gap-[9px] px-2.5 border border-[var(--line)] rounded bg-[var(--surface)]";
                row.bar.style.opacity = "";
                row.name.className = "flex-1 min-w-0 font-semibold text-xs text-[var(--ink)] overflow-hidden text-ellipsis whitespace-nowrap";
                row.meta.className = "flex-none text-[11px] font-bold text-[var(--green)] tabular-nums";
                row.meta.textContent = place;
            } else {
                row.li.className = "h-[26px] flex items-center gap-[9px] px-2.5 border border-[var(--line)] rounded bg-[var(--surface)]";
                row.bar.style.opacity = "";
                row.name.className = "flex-1 min-w-0 font-semibold text-xs text-[var(--ink)] overflow-hidden text-ellipsis whitespace-nowrap";
                row.meta.className = "flex-none text-[11px] text-[var(--muted)] tabular-nums";
                row.meta.textContent = `#${entry.agent_id}`;
            }
        }
    }

    function applySnapshot(snap) {
        const arena = snap.arena;
        if (arena) {
            canvas.width = arena.width;
            canvas.height = arena.height;
        }
        const players = new Map();
        for (const p of snap.players || []) {
            players.set(p.player_id, {
                alive: p.alive,
                head: p.head || null,
                body: p.body || [],
            });
        }
        state = { arena: arena || null, players };
        // Fresh full state: placements restart. A lone survivor (late join
        // at game end, or a 1-player game) is already decided.
        placements = new Map();
        if (aliveCount() === 1) {
            for (const [id, player] of players) {
                if (player.alive) placements.set(id, ordinal(1));
            }
        }
        if (rows.size === 0) renderLegend(); else updatePresence();
        draw();
    }

    function applyDelta(delta) {
        if (!state) return; // wait for a snapshot first
        const wasAlive = new Set();
        for (const [id, player] of state.players) {
            if (player.alive) wasAlive.add(id);
        }
        for (const p of delta.players || []) {
            let player = state.players.get(p.player_id);
            if (!player) {
                player = { alive: true, head: null, body: [] };
                state.players.set(p.player_id, player);
                wasAlive.add(p.player_id);
            }
            player.alive = p.alive;
            player.head = p.head || player.head;
            for (const blob of p.new_body || []) player.body.push(blob);
        }
        // Newly dead slots place at (alive-after-death + 1); a lone
        // survivor takes 1ST immediately — no result event needed.
        for (const [id, player] of state.players) {
            if (!player.alive && wasAlive.has(id) && !placements.has(id)) {
                placements.set(id, ordinal(aliveCount() + 1));
            }
        }
        if (aliveCount() === 1) {
            for (const [id, player] of state.players) {
                if (player.alive && !placements.has(id)) placements.set(id, ordinal(1));
            }
        }
        updatePresence();
        draw();
    }

    function resetToWaiting() {
        state = null;
        lineup = new Map();
        placements = new Map();
        rows = new Map();
        if (legendEl) {
            // Muted placeholder holds the "Playing now" layout until the
            // first lineup arrives (renderLegend clears it).
            legendEl.innerHTML = "";
            const li = document.createElement("li");
            li.className = "text-[13px] text-[var(--muted)]";
            li.textContent = "No game running.";
            legendEl.appendChild(li);
        }
        drawMessage("Waiting for a game…");
    }

    resetToWaiting();

    const es = new EventSource("/spectator/watch");

    // Idle: no game running. Reset so a stale board isn't left frozen.
    es.addEventListener("waiting", resetToWaiting);

    // New match on the same connection: reset placements and rebuild the
    // legend. The board resets when the snapshot arrives.
    es.addEventListener("lineup", (e) => {
        const data = JSON.parse(e.data);
        lineup = new Map();
        for (const s of data.slots || []) {
            lineup.set(s.slot, { agent_id: s.agent_id, name: s.name });
        }
        placements = new Map();
        renderLegend();
    });

    es.addEventListener("snapshot", (e) => applySnapshot(JSON.parse(e.data)));
    es.addEventListener("delta", (e) => applyDelta(JSON.parse(e.data)));
}

window.init_spectator = init_spectator;
