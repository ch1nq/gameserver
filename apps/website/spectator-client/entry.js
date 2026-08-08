// Live spectator client (bundled to ../static/spectator.js by
// `just gen-spectator-client`). Speaks gRPC-Web to /spectator.Spectator/Watch,
// which the website relays from the current game host's GameHost.WatchGame
// stream. The stream yields one full snapshot followed by per-tick deltas; we
// accumulate them and render the Achtung curve to a canvas.

const { SpectatorClient } = require("./gen/spectator_grpc_web_pb.js");
const { WatchRequest } = require("./gen/spectator_pb.js");
const { SpectatorSnapshot, SpectatorDelta } = require("./gen/achtung_spectator_pb.js");

const RECONNECT_MS = 1500;

// Distinct-ish colors per player slot; wraps if there are more players.
const PLAYER_COLORS = [
    "#ff4d4d", "#4dd2ff", "#7cff4d", "#ffd24d",
    "#c04dff", "#ff8c4d", "#4dffbf", "#ff4da6",
];

function playerColor(id) {
    return PLAYER_COLORS[id % PLAYER_COLORS.length];
}

function blobToObj(b) {
    return { x: b.getX(), y: b.getY(), size: b.getSize() };
}

function init_spectator(canvasId) {
    const canvas = document.getElementById(canvasId);
    const ctx = canvas.getContext("2d");
    const client = new SpectatorClient(window.location.origin);

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
        const arena = snap.getArena();
        if (arena) {
            canvas.width = arena.getWidth();
            canvas.height = arena.getHeight();
        }
        const players = new Map();
        for (const p of snap.getPlayersList()) {
            players.set(p.getPlayerId(), {
                alive: p.getAlive(),
                head: p.getHead() ? blobToObj(p.getHead()) : null,
                body: p.getBodyList().map(blobToObj),
            });
        }
        state = { arena: arena ? { width: arena.getWidth(), height: arena.getHeight() } : null, players };
        draw();
    }

    function applyDelta(delta) {
        if (!state) return; // wait for a snapshot first
        for (const p of delta.getPlayersList()) {
            let player = state.players.get(p.getPlayerId());
            if (!player) {
                player = { alive: true, head: null, body: [] };
                state.players.set(p.getPlayerId(), player);
            }
            player.alive = p.getAlive();
            player.head = p.getHead() ? blobToObj(p.getHead()) : player.head;
            for (const blob of p.getNewBodyList()) player.body.push(blobToObj(blob));
        }
        draw();
    }

    function connect() {
        state = null;
        drawMessage("Waiting for a game…");
        const stream = client.watch(new WatchRequest(), {});
        stream.on("data", (frame) => {
            const payload = frame.getPayload_asU8();
            if (frame.getIsSnapshot()) {
                applySnapshot(SpectatorSnapshot.deserializeBinary(payload));
            } else {
                applyDelta(SpectatorDelta.deserializeBinary(payload));
            }
        });
        // Between games the relay returns UNAVAILABLE and the stream ends
        // quickly; reconnect so the next game is picked up.
        stream.on("error", () => setTimeout(connect, RECONNECT_MS));
        stream.on("end", () => setTimeout(connect, RECONNECT_MS));
    }

    connect();
}

window.init_spectator = init_spectator;
