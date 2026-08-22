//! Browser-facing spectator stream, served as Server-Sent Events.
//!
//! One background task maintains a single gRPC `WatchGame` stream to the
//! current game host and fans every `SpectatorFrame` out to all connected SSE
//! clients via a `tokio::sync::broadcast` channel. Late-joining clients receive
//! the full frame history buffered since the last snapshot, then switch to the
//! live broadcast — so they get a consistent view without the game host ever
//! opening more than one outbound stream to the website.

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use coordinator::SpectatorRegistry;
use coordinator::game_host::WatchGameRequest;
use coordinator::game_host::game_host_client::GameHostClient;
use coordinator::spectator_frame::SpectatorFrame;
use prost::Message;
use tokio::sync::broadcast;
use tokio::time::sleep;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};

/// Reconnect delay (`retry:`) handed to the browser's `EventSource`.
const RECONNECT_MS: u64 = 1500;

/// Broadcast buffer: must be large enough to hold a full game's frames so that
/// slow subscribers are not lagged off mid-game.
const BROADCAST_BUFFER: usize = 4096;

type EventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

// Decoded achtung spectator payloads with `serde::Serialize` derived in build.rs.
mod achtung {
    tonic::include_proto!("achtung.spectator");
}

/// In-process hub that holds the frame history for the current game and a live
/// broadcast channel. All SSE clients subscribe here.
struct SpectatorHub {
    /// Every frame from the current game, for clients that connect mid-game.
    history: Vec<SpectatorFrame>,
    /// `Some(frame)` during a game; `None` is the game-over sentinel.
    sender: broadcast::Sender<Option<SpectatorFrame>>,
    /// True while the background task is actively receiving frames from the
    /// game host. Checked atomically with `sender.subscribe()` so a tab that
    /// connects in the gap between "game over sentinel sent" and "registry
    /// guard clears" doesn't get stuck waiting on a broadcast that will never
    /// deliver.
    game_active: bool,
}

impl SpectatorHub {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_BUFFER);
        Self {
            history: Vec::new(),
            sender,
            game_active: false,
        }
    }
}

/// Shared state for the spectator SSE route.
#[derive(Clone)]
pub struct SpectatorState {
    registry: SpectatorRegistry,
    hub: Arc<std::sync::Mutex<SpectatorHub>>,
}

impl SpectatorState {
    pub fn new(registry: SpectatorRegistry) -> Self {
        let hub = Arc::new(std::sync::Mutex::new(SpectatorHub::new()));
        let state = Self { registry, hub };
        tokio::spawn(run_broadcaster(state.clone()));
        state
    }
}

/// Background task: opens exactly one gRPC stream per game and broadcasts every
/// frame to the hub. Polls the registry every 100 ms between games.
async fn run_broadcaster(state: SpectatorState) {
    loop {
        // Wait for a game to start.
        let addr = loop {
            if let Some(addr) = state.registry.read().await.clone() {
                break addr;
            }
            sleep(Duration::from_millis(100)).await;
        };

        let mut client = match GameHostClient::connect(addr.clone()).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("spectator broadcaster: cannot connect to {addr}: {e}");
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let stream = match client.watch_game(WatchGameRequest {}).await {
            Ok(r) => r.into_inner(),
            Err(e) => {
                tracing::warn!("spectator broadcaster: watch_game failed: {e}");
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        state.hub.lock().unwrap().game_active = true;

        tokio::pin!(stream);
        while let Some(result) = stream.next().await {
            let frame = match result {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!("spectator broadcaster: stream error: {e}");
                    break;
                }
            };
            // Lock, push to history, and broadcast atomically so that a
            // subscriber who calls hub.lock() between these two steps never
            // sees a gap — the same guarantee grpc.rs achieves for gRPC clients.
            let mut hub = state.hub.lock().unwrap();
            if frame.is_snapshot {
                hub.history.clear();
            }
            hub.history.push(frame.clone());
            let _ = hub.sender.send(Some(frame));
        }

        // Game over: mark inactive, signal waiting SSE clients, and reset.
        // Setting game_active = false happens atomically with the sentinel
        // send, so any subscriber who calls hub.lock() after this point sees
        // the correct state.
        {
            let mut hub = state.hub.lock().unwrap();
            hub.game_active = false;
            hub.history.clear();
            let _ = hub.sender.send(None);
        }

        // Wait until the registry clears (or moves to a new address) so the
        // outer loop doesn't immediately reconnect to the same ended game.
        while state.registry.read().await.as_deref() == Some(addr.as_str()) {
            sleep(Duration::from_millis(100)).await;
        }
    }
}

/// Router for the browser-facing spectator SSE endpoint.
pub fn router(state: SpectatorState) -> Router {
    Router::new()
        .route("/spectator/watch", get(watch))
        .with_state(state)
}

/// `GET /spectator/watch` — streams spectator frames as JSON SSE events.
///
/// Emits a leading `retry:` directive, then either a `waiting` event (no game
/// running) or a sequence of `snapshot` / `delta` events sourced from the
/// in-process hub. The stream ends when the game ends; the browser's `EventSource`
/// reconnects automatically after the `retry:` delay.
async fn watch(
    State(state): State<SpectatorState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let retry = Event::default().retry(Duration::from_millis(RECONNECT_MS));
    let lead = tokio_stream::once(Ok(retry));

    // Subscribe, clone history, and read game_active under the same lock.
    // This ensures a tab that connects just after the game-over sentinel is
    // sent sees game_active=false and emits "waiting" instead of hanging on a
    // broadcast that will never deliver (the sentinel was sent before subscribe).
    let (history, receiver, game_active) = {
        let hub = state.hub.lock().unwrap();
        (hub.history.clone(), hub.sender.subscribe(), hub.game_active)
    };
    // receiver is only used in the else branch; drop it early when waiting.
    let body: EventStream = if !game_active {
        drop(receiver);
        Box::pin(waiting())
    } else {
        let history_events = tokio_stream::iter(
            history
                .into_iter()
                .filter_map(|f| frame_to_event(f).map(Ok)),
        );
        // `None` sentinel or a lag error both end the stream so the browser
        // reconnects and picks up a fresh history from the next game.
        let live = BroadcastStream::new(receiver).map_while(|r| match r {
            Ok(Some(frame)) => frame_to_event(frame).map(Ok),
            _ => None,
        });
        Box::pin(history_events.chain(live))
    };

    Sse::new(lead.chain(body)).keep_alive(KeepAlive::default())
}

/// One-shot stream that tells the browser no game is running.
fn waiting() -> impl Stream<Item = Result<Event, Infallible>> {
    tokio_stream::once(Ok(Event::default().event("waiting").data("{}")))
}

/// Decode one `SpectatorFrame` into an SSE event, or `None` on payload error.
fn frame_to_event(frame: SpectatorFrame) -> Option<Event> {
    let payload = frame.payload.as_slice();
    let (name, json) = if frame.is_snapshot {
        let snap = achtung::SpectatorSnapshot::decode(payload).ok()?;
        ("snapshot", serde_json::to_string(&snap).ok()?)
    } else {
        let delta = achtung::SpectatorDelta::decode(payload).ok()?;
        ("delta", serde_json::to_string(&delta).ok()?)
    };
    Some(Event::default().event(name).data(json))
}
