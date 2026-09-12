// Live spectator client. Hand-written (no build step): speaks Server-Sent
// Events to /spectator/watch, which the website relays from the current game
// host's GameHost.WatchGame stream, decoding the protobuf payload to JSON. The
// stream yields `lineup` when a match starts, one `snapshot` event followed by
// per-tick `delta` events during play, and a terminal `result` event with
// placements when the game ends; we accumulate frames and render the Achtung
// curve to a canvas.
//
// The SSE connection stays open across games (with keep-alive comments), so
// there is no manual reconnect/backoff logic here: `waiting` resets to the
// waiting screen, the next `lineup` starts a new game on the same connection.

// Distinct-ish colors per player slot; wraps if there are more players.
// Must match the slot order of the `lineup` event (slot i == player_id i).
const PLAYER_COLORS = [
    "#ff4d4d", "#4dd2ff", "#7cff4d", "#ffd24d",
    "#c04dff", "#ff8c4d", "#4dffbf", "#ff4da6",
];

function playerColor(id) {
    return PLAYER_COLORS[id % PLAYER_COLORS.length];
}

function init_spectator(canvasId) {
    const canvas = document.getElementById(canvasId);
    const ctx = canvas.getContext("2d");
    const tickEl = document.getElementById("spectator-tick");
    const legendEl = document.getElementById("spectator-legend");
    const resultEl = document.getElementById("spectator-result");

    // { arena: {width,height}, players: Map<id, {alive, head, body:[]}> }
    let state = null;
    // slot -> {agent_id, name}
    let lineup = new Map();
    // Persistent trail layer: trails are append-only, so deltas paint only
    // their new blobs here; each frame composites trail + heads. Snapshot
    // repaints it from scratch.
    const trailCanvas = document.createElement("canvas");
    const trailCtx = trailCanvas.getContext("2d");
    let arenaW = 0;
    let arenaH = 0;
    let paintQueued = false;

    function paintBlob(target, blob) {
        target.beginPath();
        target.arc(blob.x, blob.y, blob.size, 0, 2 * Math.PI);
        target.fill();
    }

    function setupCanvases(arena) {
        const dpr = window.devicePixelRatio || 1;
        arenaW = arena.width;
        arenaH = arena.height;
        canvas.width = arenaW * dpr;
        canvas.height = arenaH * dpr;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        trailCanvas.width = arenaW * dpr;
        trailCanvas.height = arenaH * dpr;
        trailCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }

    function repaintTrailFromState() {
        trailCtx.save();
        trailCtx.setTransform(1, 0, 0, 1, 0, 0);
        trailCtx.clearRect(0, 0, trailCanvas.width, trailCanvas.height);
        trailCtx.restore();
        for (const [id, player] of state.players) {
            trailCtx.fillStyle = playerColor(id);
            for (const blob of player.body) paintBlob(trailCtx, blob);
        }
    }

    function drawMessage(text) {
        ctx.save();
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.fillStyle = "#000033";
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.restore();
        ctx.fillStyle = "#8890b5";
        ctx.font = "20px sans-serif";
        ctx.fillText(text, 20, 36);
    }

    function paint() {
        paintQueued = false;
        if (!state) return;
        ctx.save();
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.fillStyle = "#000033";
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.restore();
        if (arenaW > 0 && arenaH > 0) ctx.drawImage(trailCanvas, 0, 0, arenaW, arenaH);
        for (const player of state.players.values()) {
            if (player.alive && player.head) {
                ctx.fillStyle = "#ffffff";
                paintBlob(ctx, player.head);
            }
        }
    }

    function requestPaint() {
        if (paintQueued) return;
        paintQueued = true;
        requestAnimationFrame(paint);
    }

    function setTick(tick) {
        if (tick === undefined || tick === null) return;
        if (tickEl) tickEl.textContent = `Tick ${tick}`;
    }

    function renderLegend() {
        if (!legendEl) return;
        legendEl.innerHTML = "";
        const slots = [...lineup.entries()].sort((a, b) => a[0] - b[0]);
        for (const [slot, entry] of slots) {
            const li = document.createElement("li");
            li.className = "flex items-center gap-2";
            const dot = document.createElement("span");
            dot.className = "inline-block h-3 w-3 rounded-full";
            dot.style.backgroundColor = playerColor(slot);
            const label = document.createElement("span");
            label.textContent = `${entry.name} (#${entry.agent_id})`;
            li.appendChild(dot);
            li.appendChild(label);
            legendEl.appendChild(li);
        }
    }

    function clearResult() {
        if (!resultEl) return;
        resultEl.innerHTML = "";
        resultEl.classList.add("hidden");
    }

    function showResult(result) {
        if (!resultEl) return;
        resultEl.innerHTML = "";
        const title = document.createElement("div");
        title.className = "font-semibold mb-1";
        const placements = [...(result.placements || [])].sort((a, b) => a.position - b.position);
        if (result.error) {
            title.textContent = `Game failed: ${result.error}`;
        } else if (placements.length > 0) {
            title.textContent = `Winner: ${winnerNameById(placements[0].agent_id)}`;
        } else {
            title.textContent = "Game over";
        }
        resultEl.appendChild(title);
        const list = document.createElement("ol");
        list.className = "list-decimal ml-5";
        for (const p of placements) {
            const item = document.createElement("li");
            item.textContent = `${winnerNameById(p.agent_id)} — place ${p.position} (score ${p.score})`;
            list.appendChild(item);
        }
        if (placements.length > 0) resultEl.appendChild(list);
        resultEl.classList.remove("hidden");
    }

    function winnerNameById(agentId) {
        for (const entry of lineup.values()) {
            if (entry.agent_id === agentId) return entry.name;
        }
        return `#${agentId}`;
    }

    function applySnapshot(snap) {
        const arena = snap.arena;
        if (arena) setupCanvases(arena);
        const players = new Map();
        for (const p of snap.players || []) {
            players.set(p.player_id, {
                alive: p.alive,
                head: p.head || null,
                body: p.body || [],
            });
        }
        state = { arena: arena || null, players };
        if (arena) repaintTrailFromState();
        setTick(snap.tick);
        requestPaint();
    }

    function applyDelta(delta) {
        if (!state) return; // wait for a snapshot first
        for (const p of delta.players || []) {
            let player = state.players.get(p.player_id);
            if (!player) {
                player = { alive: true, head: null, body: [] };
                state.players.set(p.player_id, player);
            }
            player.alive = p.alive;
            player.head = p.head || player.head;
            if (p.new_body && p.new_body.length > 0) {
                trailCtx.fillStyle = playerColor(p.player_id);
                for (const blob of p.new_body) {
                    player.body.push(blob);
                    paintBlob(trailCtx, blob);
                }
            }
        }
        setTick(delta.tick);
        requestPaint();
    }

    function resetToWaiting() {
        state = null;
        lineup = new Map();
        trailCtx.save();
        trailCtx.setTransform(1, 0, 0, 1, 0, 0);
        trailCtx.clearRect(0, 0, trailCanvas.width, trailCanvas.height);
        trailCtx.restore();
        if (legendEl) legendEl.innerHTML = "";
        if (tickEl) tickEl.textContent = "Waiting for a game…";
        clearResult();
        drawMessage("Waiting for a game…");
    }

    resetToWaiting();

    const es = new EventSource("/spectator/watch");

    // Idle: no game running. Reset so a stale board isn't left frozen.
    es.addEventListener("waiting", resetToWaiting);

    // New match on the same connection: rebuild the legend, clear the old
    // result overlay. The board resets when the snapshot arrives.
    es.addEventListener("lineup", (e) => {
        const data = JSON.parse(e.data);
        lineup = new Map();
        for (const s of data.slots || []) {
            lineup.set(s.slot, { agent_id: s.agent_id, name: s.name });
        }
        renderLegend();
        clearResult();
        if (tickEl) tickEl.textContent = "Game starting…";
    });

    es.addEventListener("snapshot", (e) => applySnapshot(JSON.parse(e.data)));
    es.addEventListener("delta", (e) => applyDelta(JSON.parse(e.data)));

    // Terminal result: freeze the final board behind the overlay until the
    // next `lineup` (or `waiting`) clears it.
    es.addEventListener("result", (e) => showResult(JSON.parse(e.data)));
}

window.init_spectator = init_spectator;
