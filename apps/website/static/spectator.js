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
//
// Presentation lives outside this file: row markup comes from
// `#spectator-legend-template` / `#spectator-empty-template` in landing.rs,
// colors and dimming live in app.css (`--player-*`, `data-slot`, state
// classes). This script only fills in data (texts, `data-slot`) and toggles
// state classes (`is-dead` / `is-winner`); it never writes inline colors.

function init_spectator(canvasId) {
    const canvas = document.getElementById(canvasId);
    const ctx = canvas.getContext("2d");
    const legendEl = document.getElementById("spectator-legend");
    const rowTemplate = document.getElementById("spectator-legend-template");
    const emptyTemplate = document.getElementById("spectator-empty-template");

    // Palette + board colors resolve from the app.css `:root` tokens (single
    // source of truth). Canvas needs concrete strings — `var()` won't paint —
    // so read the computed values once at init. Fallbacks match app.css for
    // robustness (e.g. stylesheet not yet applied).
    const cssVars = getComputedStyle(document.documentElement);
    function cssVar(name, fallback) {
        const value = cssVars.getPropertyValue(name).trim();
        return value || fallback;
    }
    const PLAYER_COLORS = [
        cssVar("--player-0", "#5fc9ff"), cssVar("--player-1", "#ff6fa8"),
        cssVar("--player-2", "#ffd84a"), cssVar("--player-3", "#7bf0a8"),
        cssVar("--player-4", "#b78cff"), cssVar("--player-5", "#ff9a4d"),
        cssVar("--player-6", "#4de0d0"), cssVar("--player-7", "#f45b5b"),
    ];
    const BOARD_BG = cssVar("--spectator-board", "#0A0B10");
    const BOARD_MUTED = cssVar("--spectator-muted", "#8890b5");
    const HEAD_COLOR = "#ffffff";
    const MESSAGE_FONT = "20px sans-serif";

    // Must match the slot order of the `lineup` event (slot i == player_id i);
    // wraps if there are more players than palette entries.
    function playerColor(id) {
        return PLAYER_COLORS[id % PLAYER_COLORS.length];
    }

    // { arena: {width,height}, players: Map<id, {alive, head, body:[]}> }
    let state = null;
    // slot -> {agent_id, name}
    let lineup = new Map();
    // slot -> live placement ("8TH"), in death order; reset each game.
    let placements = new Map();
    // slot -> {li, meta} row elements, rebuilt per game.
    let rows = new Map();

    function drawMessage(text) {
        ctx.fillStyle = BOARD_BG;
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.fillStyle = BOARD_MUTED;
        ctx.font = MESSAGE_FONT;
        ctx.fillText(text, 20, 36);
    }

    function draw() {
        if (!state) return;
        ctx.fillStyle = BOARD_BG;
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        for (const [id, player] of state.players) {
            ctx.fillStyle = playerColor(id);
            for (const blob of player.body) {
                ctx.beginPath();
                ctx.arc(blob.x, blob.y, blob.size, 0, 2 * Math.PI);
                ctx.fill();
            }
            if (player.alive && player.head) {
                ctx.fillStyle = HEAD_COLOR;
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

    function cloneRow() {
        return rowTemplate.content.firstElementChild.cloneNode(true);
    }

    function showEmpty() {
        if (!legendEl) return;
        legendEl.replaceChildren();
        if (emptyTemplate) {
            legendEl.appendChild(emptyTemplate.content.firstElementChild.cloneNode(true));
        }
    }

    // Landing "Playing now" rows: clones the maud-owned `<template>` (color
    // bar + name + #agent_id) and fills in per-slot data. Styling (per-slot
    // color via `data-slot`, dead/winner dimming) is pure CSS in app.css.
    function renderLegend() {
        if (!legendEl || !rowTemplate) return;
        legendEl.replaceChildren();
        rows = new Map();
        const slots = [...lineup.entries()].sort((a, b) => a[0] - b[0]);
        for (const [slot, entry] of slots) {
            const li = cloneRow();
            // Presentational index only: normalized so larger games wrap the
            // 8 `--player-*` tokens defined in app.css.
            li.dataset.slot = String(slot % PLAYER_COLORS.length);
            li.querySelector(".legend-name").textContent = entry.name;
            const meta = li.querySelector(".legend-meta");
            meta.textContent = `#${entry.agent_id}`;
            legendEl.appendChild(li);
            rows.set(slot, { li, meta });
        }
        updatePresence();
    }

    // Repaints rows in place (called per frame): toggles state classes only —
    // CSS decides the visuals — and updates the meta text (data, not styling).
    function updatePresence() {
        for (const [slot, entry] of lineup) {
            const row = rows.get(slot);
            if (!row) continue;
            const player = state ? state.players.get(slot) : undefined;
            const alive = !player || player.alive;
            const place = placements.get(slot);
            row.li.classList.toggle("is-dead", !alive);
            row.li.classList.toggle("is-winner", alive && place !== undefined);
            if (!alive) {
                row.meta.textContent = place === undefined ? "–" : place;
            } else if (place !== undefined) {
                // Sole survivor: placement in green via .is-winner.
                row.meta.textContent = place;
            } else {
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
        // Muted placeholder holds the "Playing now" layout until the first
        // lineup arrives (renderLegend clears it). Markup comes from the
        // maud-owned empty template; the initial server render already
        // contains the same row.
        showEmpty();
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
