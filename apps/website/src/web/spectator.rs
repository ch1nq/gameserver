//! Browser-facing spectator stream, served as Server-Sent Events.
//!
//! Replaces the old gRPC-Web relay. The game host still produces a
//! `spectator_frame.SpectatorFrame` stream (a full snapshot followed by
//! per-tick deltas); this handler dials that stream, decodes the achtung
//! payload, and re-emits each frame as a JSON SSE event. The browser uses a
//! native `EventSource` (auto-reconnecting), so it needs no protobuf runtime.

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
use prost::Message;
use tokio::sync::Mutex;
use tokio_stream::{Stream, StreamExt};
use tonic::transport::Channel;

/// Reconnect delay (`retry:`) handed to the browser's `EventSource`. Between
/// games each connection ends immediately, so this paces the re-poll.
const RECONNECT_MS: u64 = 1500;

type EventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

/// `(address, client)` for the currently-cached game host, if any.
type CachedClient = Arc<Mutex<Option<(String, GameHostClient<Channel>)>>>;

// Decoded achtung spectator payloads (SpectatorSnapshot / SpectatorDelta),
// with `serde::Serialize` derived in build.rs so they can be emitted as JSON.
mod achtung {
    tonic::include_proto!("achtung.spectator");
}

/// Shared state for the spectator SSE route: the registry pointing at the
/// current game host, and a cached client so consecutive spectators of the same
/// game reuse one multiplexed HTTP/2 connection instead of dialing per browser.
#[derive(Clone)]
pub struct SpectatorState {
    registry: SpectatorRegistry,
    client: CachedClient,
}

impl SpectatorState {
    pub fn new(registry: SpectatorRegistry) -> Self {
        Self {
            registry,
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// A `GameHostClient` for the current game host, reusing the cached
    /// connection when the address is unchanged. `None` when no game is running.
    async fn client(&self) -> Option<GameHostClient<Channel>> {
        let addr = self.registry.read().await.clone()?;

        let mut cached = self.client.lock().await;
        if let Some((cached_addr, client)) = cached.as_ref()
            && cached_addr == &addr
        {
            // Cloning a tonic client clones its Channel, which multiplexes new
            // streams over the one existing HTTP/2 connection.
            return Some(client.clone());
        }

        // Address changed (new game) or first connect: dial and cache.
        match GameHostClient::connect(addr.clone()).await {
            Ok(client) => {
                *cached = Some((addr, client.clone()));
                Some(client)
            }
            Err(e) => {
                tracing::warn!("spectator: game host unreachable at {addr}: {e}");
                *cached = None;
                None
            }
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
/// Always responds `200 text/event-stream` so the browser's `EventSource`
/// auto-reconnects when the stream ends (a native EventSource does *not* retry
/// after an HTTP error status). Between games the stream carries a single
/// `waiting` event and ends; during a game it carries `snapshot` then `delta`
/// events with a JSON `data:` payload (decoded achtung
/// `SpectatorSnapshot` / `SpectatorDelta`). A leading `retry:` paces reconnects.
async fn watch(
    State(state): State<SpectatorState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Lead with a `retry:` directive so the reconnect interval is known even
    // when a game is running (the browser reuses it after the game ends).
    let retry = Event::default().retry(Duration::from_millis(RECONNECT_MS));
    let lead = tokio_stream::once(Ok(retry));

    let body: EventStream = match state.client().await {
        Some(mut client) => match client.watch_game(WatchGameRequest {}).await {
            Ok(resp) => {
                // Transcode each frame to a JSON SSE event. On any upstream error
                // or a payload that fails to decode, end the stream; the browser
                // reconnects and (mid-game) gets a fresh snapshot from the host.
                let events = resp
                    .into_inner()
                    .map_while(|frame| frame.ok().and_then(frame_to_event).map(Ok));
                Box::pin(events)
            }
            Err(e) => {
                tracing::warn!("spectator: watch_game failed: {e}");
                Box::pin(waiting())
            }
        },
        None => Box::pin(waiting()),
    };

    Sse::new(lead.chain(body)).keep_alive(KeepAlive::default())
}

/// A one-shot stream that tells the browser no game is running, then ends so
/// the `EventSource` reconnects after the `retry:` delay.
fn waiting() -> impl Stream<Item = Result<Event, Infallible>> {
    tokio_stream::once(Ok(Event::default().event("waiting").data("{}")))
}

/// Decode one `SpectatorFrame` into an SSE event, or `None` if its payload does
/// not decode (which ends the stream).
fn frame_to_event(frame: coordinator::spectator_frame::SpectatorFrame) -> Option<Event> {
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
