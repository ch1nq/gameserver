// Live spectator client. Hand-written (no build step): speaks Server-Sent
// Events to /spectator/watch, which the website relays from the current game
// host's GameHost.WatchGame stream, decoding the protobuf payload to JSON. The
// stream yields one `snapshot` event followed by per-tick `delta` events; we
// accumulate them and render the Achtung curve to a canvas.
//
// The browser's native EventSource reconnects automatically when the stream
// ends (between games the server sends a `waiting` event and closes), so there
// is no manual reconnect/backoff logic here.

// Distinct-ish colors per player slot; wraps if there are more players.
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

    // { arena: {width,height}, players: Map<id, {alive, head, body:[]}> }
    let state = null;

    function drawMessage(text) {
        ctx.fillStyle = "#000033";
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.fillStyle = "#8890b5";
        ctx.font = "20px sans-serif";
        ctx.fillText(text, 20, 36);
    }

    function draw() {
        if (!state) return;
        ctx.fillStyle = "#000033";
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
        draw();
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
            for (const blob of p.new_body || []) player.body.push(blob);
        }
        draw();
    }

    drawMessage("Waiting for a game…");

    const es = new EventSource("/spectator/watch");

    // Between games the server closes the stream after a `waiting` event; reset
    // to the waiting screen so a stale board isn't left frozen on screen.
    es.addEventListener("waiting", () => {
        state = null;
        drawMessage("Waiting for a game…");
    });

    es.addEventListener("snapshot", (e) => applySnapshot(JSON.parse(e.data)));
    es.addEventListener("delta", (e) => applyDelta(JSON.parse(e.data)));
}

window.init_spectator = init_spectator;
