//! Browser-facing spectator stream, served as Server-Sent Events.
//!
//! One background task maintains a single gRPC `WatchGame` stream to the
//! current game host and fans every event out to all connected SSE clients via
//! a `tokio::sync::broadcast` channel. Late-joining clients replay the buffered
//! lineup + frame history, then switch to the live broadcast — so they get a
//! consistent view without the game host ever opening more than one outbound
//! stream to the website.
//!
//! The SSE connection is long-lived: it stays open across games. The relay
//! emits `lineup` when a match starts, `snapshot`/`delta` frames during play,
//! and a terminal `result` when the match ends; idle tabs receive a single
//! `waiting` event and then wait on the same connection (with `KeepAlive`
//! comments) instead of reconnecting every `retry:` interval.

use std::collections::VecDeque;
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
use coordinator::game_host::WatchGameRequest;
use coordinator::game_host::game_host_client::GameHostClient;
use coordinator::spectator_frame::SpectatorFrame;
use coordinator::{GameHostAddr, LineupEntry, SpectatorRegistry, SpectatorResult};
use prost::Message;
use tokio::sync::{RwLock, broadcast};
use tokio::time::sleep;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};

/// Reconnect delay (`retry:`) handed to the browser's `EventSource`. Only used
/// when the stream actually drops (network error, lagged subscriber) — game
/// boundaries no longer close the stream.
const RECONNECT_MS: u64 = 1500;

/// Broadcast buffer: must be large enough to hold a full game's frames so that
/// slow subscribers are not lagged off mid-game.
///
/// Note the game host's own `SPECTATOR_BUFFER` (1024) is deliberately smaller:
/// that hop has exactly one subscriber (this relay) and drops it fast so the
/// relay reconnects for a fresh snapshot, while this hop fans out to many
/// browsers and tolerates slower viewers.
const BROADCAST_BUFFER: usize = 4096;

/// Frame history retained per game for late joiners. Bounded so a very long
/// game can't grow memory without limit; the leading snapshot is pinned so
/// late joiners always have base state (very late joiners of a huge game may
/// see partial trails — live viewers are unaffected).
const HISTORY_CAP: usize = 4096;

type EventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

// Decoded achtung spectator payloads with `serde::Serialize` derived in build.rs.
mod achtung {
    tonic::include_proto!("achtung.spectator");
}

/// One event fanned out to every SSE client. Frames are decoded from protobuf
/// to JSON **once** in the broadcaster; subscribers share the encoded string.
#[derive(Clone, Debug)]
enum HubEvent {
    /// Match lineup, sent when a game starts (slot order == player slots).
    Lineup(Vec<LineupEntry>),
    /// A game frame; `is_snapshot == true` renders as `snapshot`, else `delta`.
    Frame { is_snapshot: bool, json: String },
    /// Terminal placements, published by the coordinator after the game ends.
    Result(SpectatorResult),
}

/// In-process hub that holds the lineup + frame history for the current game
/// and a live broadcast channel. All SSE clients subscribe here.
struct SpectatorHub {
    /// Current (or most recent) match lineup for the color legend.
    lineup: Vec<LineupEntry>,
    /// Frame events since the last snapshot, for clients that connect mid-game.
    history: VecDeque<HubEvent>,
    /// Terminal result of the most recent game, retained until the next lineup.
    last_result: Option<SpectatorResult>,
    sender: broadcast::Sender<Arc<HubEvent>>,
    /// True while the background task is actively receiving frames from the
    /// game host.
    game_active: bool,
}

impl SpectatorHub {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_BUFFER);
        Self {
            lineup: Vec::new(),
            history: VecDeque::new(),
            last_result: None,
            sender,
            game_active: false,
        }
    }

    /// Push a frame to the bounded history, pinning the leading snapshot.
    fn push_frame(&mut self, event: HubEvent) {
        let is_snapshot = matches!(
            &event,
            HubEvent::Frame {
                is_snapshot: true,
                ..
            }
        );
        if is_snapshot {
            self.history.clear();
        }
        self.history.push_back(event);
        while self.history.len() > HISTORY_CAP {
            // Evict the oldest delta but keep the leading snapshot pinned.
            let pin_snapshot = self.history.len() > 1
                && matches!(
                    &self.history[0],
                    HubEvent::Frame {
                        is_snapshot: true,
                        ..
                    }
                );
            if pin_snapshot {
                self.history.remove(1);
            } else {
                self.history.pop_front();
            }
        }
    }
}

/// Shared state for the spectator SSE route.
#[derive(Clone)]
pub struct SpectatorState {
    registry: SpectatorRegistry,
    hub: Arc<RwLock<SpectatorHub>>,
}

impl SpectatorState {
    pub fn new(registry: SpectatorRegistry) -> Self {
        let hub = Arc::new(RwLock::new(SpectatorHub::new()));
        let state = Self { registry, hub };
        tokio::spawn(run_broadcaster(state.clone()));
        state
    }
}

/// Background task: opens exactly one gRPC stream per game and broadcasts every
/// frame to the hub. Polls the registry every 100 ms between games. A stream
/// that ends mid-game (same address, no published result) is re-watched after
/// a beat — the host re-snapshots on subscribe, so viewers self-heal; only a
/// published result or a changed address ends the game.
async fn run_broadcaster(state: SpectatorState) {
    // Address of the game whose lineup was last published. Guards against
    // re-broadcasting `lineup` (and wiping history) when recovering from a
    // transient error on the same game.
    let mut published_for: Option<GameHostAddr> = None;

    loop {
        // Wait for a game to start.
        let (addr, lineup) = loop {
            let m = state.registry.read().await;
            if let Some(addr) = m.addr.clone() {
                break (addr, m.lineup.clone());
            }
            drop(m);
            sleep(Duration::from_millis(100)).await;
        };

        // New match: publish the lineup, reset per-game state. Skipped when
        // recovering on the same game. The write lock is held across the state
        // update and the broadcast send so a subscriber replaying under the
        // read lock never sees a gap.
        if published_for.as_ref() != Some(&addr) {
            {
                let mut hub = state.hub.write().await;
                hub.lineup = lineup.clone();
                hub.history.clear();
                hub.last_result = None;
                hub.game_active = true;
                let _ = hub.sender.send(Arc::new(HubEvent::Lineup(lineup.clone())));
            }
            published_for = Some(addr.clone());
        }

        pump_stream(&state, &addr).await;

        // The stream ended. Distinguish game-over from a transient blip: the
        // coordinator publishes the terminal result into the registry *before*
        // tearing the host down, so a present result means game over — even if
        // the guard already cleared the address (drop order is not synchronized
        // with this task, so the address must not gate the result).
        if let Some(r) = state.registry.read().await.last_result.clone() {
            let mut hub = state.hub.write().await;
            hub.game_active = false;
            // Kept board history is the backdrop for the result overlay; only
            // send on change so a flap never double-emits.
            if hub.last_result.as_ref() != Some(&r) {
                hub.last_result = Some(r.clone());
                let _ = hub.sender.send(Arc::new(HubEvent::Result(r)));
            }
            published_for = None;
        } else if state.registry.read().await.addr.as_ref() != Some(&addr) {
            // Game went away with no result (failed/cancelled before publish).
            // Mark inactive but keep the board; there is nothing to overlay.
            state.hub.write().await.game_active = false;
            published_for = None;
        } else {
            // Same address, no result: transient blip mid-game. Loop back and
            // re-watch WITHOUT republishing lineup or clearing history.
            sleep(Duration::from_secs(1)).await;
            continue;
        }

        // Wait until the registry moves to a new address so the outer loop
        // doesn't reconnect to the same ended game. A result landing late
        // (e.g. a slow final poll) is still forwarded. SSE connections stay
        // open throughout; there is no close/retry churn.
        loop {
            let (moved_on, late_result) = {
                let m = state.registry.read().await;
                (m.addr.as_ref() != Some(&addr), m.last_result.clone())
            };
            if let Some(r) = late_result {
                let mut hub = state.hub.write().await;
                if hub.last_result.as_ref() != Some(&r) {
                    hub.last_result = Some(r.clone());
                    let _ = hub.sender.send(Arc::new(HubEvent::Result(r)));
                }
            }
            if moved_on {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    }
}

/// Open `watch_game` on `addr` and forward frames until the stream ends or the
/// coordinator publishes the terminal result (a finished host stops sending
/// but does not always close the stream promptly — the destroy that breaks it
/// can lag the result). Mid-game hub state is untouched here: a snapshot frame
/// resets history on its own via [`SpectatorHub::push_frame`].
async fn pump_stream(state: &SpectatorState, addr: &GameHostAddr) {
    let mut client = match GameHostClient::connect(addr.to_string()).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("spectator broadcaster: cannot connect to {addr}: {e}");
            sleep(Duration::from_secs(1)).await;
            return;
        }
    };

    let stream = match client.watch_game(WatchGameRequest {}).await {
        Ok(r) => r.into_inner(),
        Err(e) => {
            tracing::warn!("spectator broadcaster: watch_game failed: {e}");
            sleep(Duration::from_secs(1)).await;
            return;
        }
    };

    tokio::pin!(stream);
    // Also watch for the terminal result: the coordinator publishes it before
    // tearing the host down, so its presence ends the game even while this
    // stream is still open.
    let mut result_poll = tokio::time::interval(Duration::from_millis(100));
    result_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            msg = stream.next() => {
                let Some(result) = msg else { break };
                let frame = match result {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::warn!("spectator broadcaster: stream error: {e}");
                        break;
                    }
                };
                // Decode once here; every subscriber shares the JSON string.
                let Some(event) = decode_frame(&frame) else {
                    tracing::warn!("spectator broadcaster: bad frame payload, skipping");
                    continue;
                };
                // Lock, push to history, and broadcast atomically so that a
                // subscriber replaying under the read lock never sees a gap.
                let mut hub = state.hub.write().await;
                hub.push_frame(event.clone());
                let _ = hub.sender.send(Arc::new(event));
            }
            _ = result_poll.tick() => {
                if state.registry.read().await.last_result.is_some() {
                    break;
                }
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

/// `GET /spectator/watch` — streams spectator events as JSON SSE events.
///
/// Emits a leading `retry:` directive, replays the current `lineup` /
/// `snapshot` / `delta` / `result` state, then forwards live events on the
/// same connection forever: `lineup` at game start, `snapshot` + `delta`
/// frames during play, `result` at game end. Idle tabs get one `waiting`
/// event and then wait (with keep-alive comments) for the next game instead
/// of reconnecting. Only a lagged-out subscriber's stream ends, so the browser
/// reconnects and picks up a fresh replay.
async fn watch(
    State(state): State<SpectatorState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let retry = Event::default().retry(Duration::from_millis(RECONNECT_MS));
    let lead = tokio_stream::once(Ok(retry));

    // Subscribe, replay buffered state, and read liveness under the same read
    // lock. The broadcaster mutates + sends under the write lock, so this
    // replay can neither miss an event nor duplicate one.
    let (replay, receiver, is_idle) = {
        let hub = state.hub.read().await;
        let mut replay: Vec<Arc<HubEvent>> = Vec::new();
        if !hub.lineup.is_empty() {
            replay.push(Arc::new(HubEvent::Lineup(hub.lineup.clone())));
        }
        replay.extend(hub.history.iter().cloned().map(Arc::new));
        if !hub.game_active
            && let Some(r) = hub.last_result.clone()
        {
            replay.push(Arc::new(HubEvent::Result(r)));
        }
        let is_idle = hub.lineup.is_empty() && hub.history.is_empty() && hub.last_result.is_none();
        let receiver = hub.sender.subscribe();
        (replay, receiver, is_idle)
    };
    let replay_events = tokio_stream::iter(
        replay
            .into_iter()
            .filter_map(|e| hub_event_to_event(&e).map(Ok)),
    );

    // A lag error ends this stream so the browser reconnects and picks up a
    // fresh replay; game boundaries never end it.
    let live = BroadcastStream::new(receiver).map_while(|r| match r {
        Ok(event) => hub_event_to_event(&event).map(Ok),
        Err(_) => None,
    });

    // Pure idle (nothing buffered, no game running): tell the browser once,
    // then hold the connection open for the next game's events.
    let body: EventStream = if is_idle {
        Box::pin(tokio_stream::once(Ok(Event::default().event("waiting").data("{}"))).chain(live))
    } else {
        Box::pin(replay_events.chain(live))
    };

    Sse::new(lead.chain(body)).keep_alive(KeepAlive::default())
}

/// Decode one `SpectatorFrame` into a hub event (protobuf → JSON, once per
/// frame), or `None` on payload error.
fn decode_frame(frame: &SpectatorFrame) -> Option<HubEvent> {
    let payload = frame.payload.as_slice();
    let (is_snapshot, json) = if frame.is_snapshot {
        let snap = achtung::SpectatorSnapshot::decode(payload).ok()?;
        (true, serde_json::to_string(&snap).ok()?)
    } else {
        let delta = achtung::SpectatorDelta::decode(payload).ok()?;
        (false, serde_json::to_string(&delta).ok()?)
    };
    Some(HubEvent::Frame { is_snapshot, json })
}

/// Render one hub event as an SSE event. Pure formatting — no decoding.
fn hub_event_to_event(event: &HubEvent) -> Option<Event> {
    match event {
        HubEvent::Lineup(lineup) => {
            let json = serde_json::json!({ "slots": lineup }).to_string();
            Some(Event::default().event("lineup").data(json))
        }
        HubEvent::Frame { is_snapshot, json } => {
            let name = if *is_snapshot { "snapshot" } else { "delta" };
            Some(Event::default().event(name).data(json.clone()))
        }
        HubEvent::Result(result) => {
            let json = serde_json::to_string(result).ok()?;
            Some(Event::default().event("result").data(json))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coordinator::{AgentPlacement, LineupEntry, SpectatorResult};

    fn snapshot_frame(tick: u64) -> SpectatorFrame {
        let snap = achtung::SpectatorSnapshot {
            tick,
            arena: Some(achtung::ArenaConfig {
                width: 1000,
                height: 1000,
            }),
            players: vec![achtung::PlayerBody {
                player_id: 0,
                alive: true,
                head: Some(achtung::Blob {
                    x: 1.0,
                    y: 2.0,
                    size: 3.0,
                }),
                body: vec![],
            }],
        };
        SpectatorFrame {
            tick,
            is_snapshot: true,
            payload: snap.encode_to_vec(),
        }
    }

    fn delta_frame(tick: u64) -> SpectatorFrame {
        let delta = achtung::SpectatorDelta {
            tick,
            players: vec![achtung::PlayerDelta {
                player_id: 0,
                alive: true,
                head: Some(achtung::Blob {
                    x: 4.0,
                    y: 5.0,
                    size: 3.0,
                }),
                new_body: vec![],
            }],
        };
        SpectatorFrame {
            tick,
            is_snapshot: false,
            payload: delta.encode_to_vec(),
        }
    }

    #[test]
    fn decode_frame_preserves_tick_in_json() {
        let event = decode_frame(&snapshot_frame(42)).expect("snapshot decodes");
        match event {
            HubEvent::Frame { is_snapshot, json } => {
                assert!(is_snapshot);
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["tick"], 42);
            }
            other => panic!("expected Frame, got {other:?}"),
        }

        let event = decode_frame(&delta_frame(43)).expect("delta decodes");
        match event {
            HubEvent::Frame { is_snapshot, json } => {
                assert!(!is_snapshot);
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["tick"], 43);
            }
            other => panic!("expected Frame, got {other:?}"),
        }
    }

    #[test]
    fn decode_frame_rejects_garbage() {
        let bad = SpectatorFrame {
            tick: 0,
            is_snapshot: true,
            payload: vec![0xff, 0xff, 0xff],
        };
        assert!(decode_frame(&bad).is_none());
    }

    #[test]
    fn push_frame_resets_on_snapshot_and_bounds_history() {
        let mut hub = SpectatorHub::new();
        hub.push_frame(decode_frame(&snapshot_frame(0)).unwrap());
        hub.push_frame(decode_frame(&delta_frame(1)).unwrap());
        assert_eq!(hub.history.len(), 2);

        // A new snapshot starts a new game: history resets.
        hub.push_frame(decode_frame(&snapshot_frame(0)).unwrap());
        assert_eq!(hub.history.len(), 1);

        // Overflow evicts oldest deltas but pins the leading snapshot.
        for tick in 1..=(HISTORY_CAP as u64 + 10) {
            hub.push_frame(decode_frame(&delta_frame(tick)).unwrap());
        }
        assert_eq!(hub.history.len(), HISTORY_CAP);
        assert!(matches!(
            &hub.history[0],
            HubEvent::Frame {
                is_snapshot: true,
                ..
            }
        ));
    }

    #[test]
    fn control_events_render_to_sse() {
        let lineup = vec![
            LineupEntry {
                slot: 0,
                agent_id: 7,
                name: "alpha".into(),
            },
            LineupEntry {
                slot: 1,
                agent_id: 9,
                name: "beta".into(),
            },
        ];
        assert!(hub_event_to_event(&HubEvent::Lineup(lineup)).is_some());

        let result = SpectatorResult {
            placements: vec![AgentPlacement {
                agent_id: 7,
                position: 1,
                score: 100,
            }],
            error: String::new(),
        };
        assert!(hub_event_to_event(&HubEvent::Result(result)).is_some());

        // Lineup/result JSON shapes are what the browser parses.
        let json = serde_json::json!({ "slots": vec![LineupEntry {
            slot: 0,
            agent_id: 7,
            name: "alpha".into(),
        }]});
        assert_eq!(json["slots"][0]["name"], "alpha");
    }

    // End-to-end: fake gRPC game hosts -> real broadcaster -> live HTTP server
    // -> raw SSE client. Proves the connection stays open across games and
    // late joiners get a full replay.
    mod e2e {
        use super::*;
        use coordinator::game_host::game_host_server::{GameHost, GameHostServer};
        use coordinator::game_host::{
            GameStatus, GetStatusRequest, StartGameRequest, StartGameResponse,
        };
        use coordinator::{AgentPlacement, GameHostAddr, SpectatorMatch};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::sync::mpsc::UnboundedReceiver;

        #[derive(Clone)]
        struct FakeBehavior {
            frames: Vec<SpectatorFrame>,
            /// Fail this many initial `watch_game` calls with Unavailable.
            fail_first: Arc<AtomicUsize>,
            /// Number of `watch_game` calls served so far.
            calls: Arc<AtomicUsize>,
            /// Keep the stream open after the frames instead of ending it.
            hang_open: bool,
        }

        impl FakeBehavior {
            fn new(frames: Vec<SpectatorFrame>) -> Self {
                // Like a real host, the stream stays open past the last frame;
                // game-over is signaled through the registry only.
                Self {
                    frames,
                    fail_first: Arc::new(AtomicUsize::new(0)),
                    calls: Arc::new(AtomicUsize::new(0)),
                    hang_open: true,
                }
            }
        }

        struct FakeHost {
            behavior: FakeBehavior,
        }

        #[tonic::async_trait]
        impl GameHost for FakeHost {
            async fn start_game(
                &self,
                _req: tonic::Request<StartGameRequest>,
            ) -> Result<tonic::Response<StartGameResponse>, tonic::Status> {
                Ok(tonic::Response::new(StartGameResponse {}))
            }

            async fn get_status(
                &self,
                _req: tonic::Request<GetStatusRequest>,
            ) -> Result<tonic::Response<GameStatus>, tonic::Status> {
                unimplemented!("relay never calls GetStatus; coordinator owns the result")
            }

            type WatchGameStream =
                Pin<Box<dyn Stream<Item = Result<SpectatorFrame, tonic::Status>> + Send>>;

            async fn watch_game(
                &self,
                _req: tonic::Request<WatchGameRequest>,
            ) -> Result<tonic::Response<Self::WatchGameStream>, tonic::Status> {
                let call = self.behavior.calls.fetch_add(1, Ordering::SeqCst);
                if call < self.behavior.fail_first.load(Ordering::SeqCst) {
                    return Err(tonic::Status::unavailable("transient blip"));
                }
                let s = tokio_stream::iter(self.behavior.frames.clone().into_iter().map(Ok));
                if self.behavior.hang_open {
                    Ok(tonic::Response::new(Box::pin(
                        s.chain(tokio_stream::pending()),
                    )))
                } else {
                    Ok(tonic::Response::new(Box::pin(s)))
                }
            }
        }

        async fn serve_fake_host(frames: Vec<SpectatorFrame>) -> String {
            serve_fake_host_with(FakeBehavior::new(frames)).await
        }

        async fn serve_fake_host_with(behavior: FakeBehavior) -> String {
            let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = probe.local_addr().unwrap().port();
            drop(probe);
            tokio::spawn(async move {
                tonic::transport::Server::builder()
                    .add_service(GameHostServer::new(FakeHost { behavior }))
                    .serve(format!("127.0.0.1:{port}").parse().unwrap())
                    .await
                    .unwrap();
            });
            format!("http://127.0.0.1:{port}")
        }

        struct Env {
            registry: SpectatorRegistry,
            sse_url: String,
        }

        /// Boot the relay + website with game 1 already published. The
        /// broadcaster picks it up on its own (no test pokes the hub).
        async fn boot(host1: String) -> Env {
            let registry: SpectatorRegistry = Arc::new(RwLock::new(SpectatorMatch::default()));
            {
                let mut m = registry.write().await;
                m.addr = Some(GameHostAddr::new(host1));
                m.lineup = vec![LineupEntry {
                    slot: 0,
                    agent_id: 7,
                    name: "alpha".into(),
                }];
            }
            let state = SpectatorState::new(registry.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let sse_url = format!("http://{}/spectator/watch", listener.local_addr().unwrap());
            tokio::spawn(async move {
                axum::serve(listener, router(state).into_make_service())
                    .await
                    .unwrap();
            });
            Env { registry, sse_url }
        }

        /// Raw SSE reader: parses `event:`/`data:` frames off one HTTP
        /// response stream. Anything after a game boundary that arrives here
        /// proves the server never closed the connection (this client never
        /// reconnects).
        fn spawn_reader(url: String) -> UnboundedReceiver<(String, String)> {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(async move {
                let mut resp = reqwest::get(&url).await.unwrap();
                let mut buf = String::new();
                // `chunk()` needs no extra reqwest features (unlike
                // `bytes_stream`) and yields each HTTP chunk as it arrives.
                while let Some(chunk) = resp.chunk().await.unwrap() {
                    buf.push_str(std::str::from_utf8(&chunk).unwrap());
                    while let Some(pos) = buf.find("\n\n") {
                        let frame: String = buf.drain(..pos + 2).collect();
                        let mut event = None;
                        let mut data = String::new();
                        for line in frame.lines() {
                            if let Some(name) = line.strip_prefix("event:") {
                                event = Some(name.trim().to_string());
                            } else if let Some(d) = line.strip_prefix("data:") {
                                if !data.is_empty() {
                                    data.push('\n');
                                }
                                data.push_str(d.trim());
                            }
                        }
                        if let Some(e) = event {
                            if tx.send((e, data)).is_err() {
                                return;
                            }
                        }
                    }
                }
            });
            rx
        }

        async fn next_event(rx: &mut UnboundedReceiver<(String, String)>) -> (String, String) {
            tokio::time::timeout(Duration::from_secs(15), rx.recv())
                .await
                .expect("timed out waiting for SSE event")
                .expect("SSE stream closed unexpectedly")
        }

        /// Skip a leading `waiting` (client connected before the broadcaster
        /// published the lineup), then expect `lineup`.
        async fn expect_lineup(rx: &mut UnboundedReceiver<(String, String)>) -> serde_json::Value {
            loop {
                let (name, data) = next_event(rx).await;
                if name == "waiting" {
                    continue;
                }
                assert_eq!(name, "lineup", "expected lineup, got {name}");
                return serde_json::from_str(&data).unwrap();
            }
        }

        async fn publish_game_async(registry: &SpectatorRegistry, addr: String, name: &str) {
            let mut m = registry.write().await;
            m.addr = Some(GameHostAddr::new(addr));
            m.lineup = vec![LineupEntry {
                slot: 0,
                agent_id: if name == "alpha" { 7 } else { 9 },
                name: name.into(),
            }];
        }

        #[tokio::test]
        async fn stream_stays_open_across_two_games_with_result() {
            let host1 =
                serve_fake_host(vec![snapshot_frame(0), delta_frame(1), delta_frame(2)]).await;
            let host2 = serve_fake_host(vec![snapshot_frame(0), delta_frame(1)]).await;
            let env = boot(host1.clone()).await;
            let mut rx = spawn_reader(env.sse_url.clone());

            // Game 1 live.
            let lineup: serde_json::Value = expect_lineup(&mut rx).await;
            assert_eq!(lineup["slots"][0]["name"], "alpha");
            let (n, d) = next_event(&mut rx).await;
            assert_eq!(n, "snapshot");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&d).unwrap()["tick"],
                0
            );
            for tick in [1, 2] {
                let (n, d) = next_event(&mut rx).await;
                assert_eq!(n, "delta");
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&d).unwrap()["tick"],
                    tick
                );
            }

            // Host stream is exhausted; coordinator publishes the result.
            env.registry.write().await.last_result = Some(SpectatorResult {
                placements: vec![
                    AgentPlacement {
                        agent_id: 7,
                        position: 1,
                        score: 2,
                    },
                    AgentPlacement {
                        agent_id: 9,
                        position: 2,
                        score: 1,
                    },
                ],
                error: String::new(),
            });
            let (n, d) = next_event(&mut rx).await;
            assert_eq!(n, "result");
            let result: serde_json::Value = serde_json::from_str(&d).unwrap();
            assert_eq!(result["placements"][0]["agent_id"], 7);
            assert_eq!(result["placements"][0]["position"], 1);

            // Game 2 starts: lineup + frames arrive on the SAME connection.
            publish_game_async(&env.registry, host2, "beta").await;
            let lineup: serde_json::Value = expect_lineup(&mut rx).await;
            assert_eq!(lineup["slots"][0]["name"], "beta");
            let (n, d) = next_event(&mut rx).await;
            assert_eq!(n, "snapshot");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&d).unwrap()["tick"],
                0
            );
            let (n, _) = next_event(&mut rx).await;
            assert_eq!(n, "delta");
        }

        #[tokio::test]
        async fn late_joiner_replays_lineup_frames_and_result() {
            let host1 = serve_fake_host(vec![snapshot_frame(0), delta_frame(1)]).await;
            let env = boot(host1).await;

            // Drain game 1 on a first client to know the game is over.
            let mut first = spawn_reader(env.sse_url.clone());
            expect_lineup(&mut first).await;
            assert_eq!(next_event(&mut first).await.0, "snapshot");
            assert_eq!(next_event(&mut first).await.0, "delta");
            env.registry.write().await.last_result = Some(SpectatorResult {
                placements: vec![AgentPlacement {
                    agent_id: 7,
                    position: 1,
                    score: 1,
                }],
                error: String::new(),
            });
            assert_eq!(next_event(&mut first).await.0, "result");

            // A tab opened now must replay everything with no new broadcasts.
            let mut late = spawn_reader(env.sse_url.clone());
            let lineup: serde_json::Value = expect_lineup(&mut late).await;
            assert_eq!(lineup["slots"][0]["name"], "alpha");
            assert_eq!(next_event(&mut late).await.0, "snapshot");
            assert_eq!(next_event(&mut late).await.0, "delta");
            let (n, d) = next_event(&mut late).await;
            assert_eq!(n, "result");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&d).unwrap()["placements"][0]["agent_id"],
                7
            );
        }

        /// The coordinator publishes the result and drops the guard (clearing
        /// the address) back-to-back; the relay must still emit the result even
        /// when the address is already gone by the time it looks.
        #[tokio::test]
        async fn result_survives_addr_clear_race() {
            let host1 = serve_fake_host(vec![snapshot_frame(0), delta_frame(1)]).await;
            let env = boot(host1).await;
            let mut rx = spawn_reader(env.sse_url.clone());

            expect_lineup(&mut rx).await;
            assert_eq!(next_event(&mut rx).await.0, "snapshot");
            assert_eq!(next_event(&mut rx).await.0, "delta");

            // Game over: result and address-clear land atomically, as they do
            // when the guard drops right after `set_result`.
            {
                let mut m = env.registry.write().await;
                m.last_result = Some(SpectatorResult {
                    placements: vec![AgentPlacement {
                        agent_id: 7,
                        position: 1,
                        score: 1,
                    }],
                    error: String::new(),
                });
                m.addr = None;
            }
            let (n, d) = next_event(&mut rx).await;
            assert_eq!(n, "result");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&d).unwrap()["placements"][0]["agent_id"],
                7
            );
        }

        /// A `watch_game` failure mid-game recovers on the same game: exactly
        /// one lineup is ever broadcast and frames resume afterwards.
        #[tokio::test]
        async fn transient_watch_failure_recovers_without_duplicate_lineup() {
            let mut behavior = FakeBehavior::new(vec![snapshot_frame(0), delta_frame(1)]);
            behavior.fail_first = Arc::new(AtomicUsize::new(1));
            let host = serve_fake_host_with(behavior).await;
            let env = boot(host).await;
            let mut rx = spawn_reader(env.sse_url.clone());

            // First watch_game fails; the relay retries and streams.
            let lineup: serde_json::Value = expect_lineup(&mut rx).await;
            assert_eq!(lineup["slots"][0]["name"], "alpha");
            assert_eq!(next_event(&mut rx).await.0, "snapshot");
            assert_eq!(next_event(&mut rx).await.0, "delta");

            // The retry window is ~2s; nothing more may arrive on this game:
            // in particular no second lineup.
            let extra = tokio::time::timeout(Duration::from_secs(4), rx.recv()).await;
            assert!(
                extra.is_err(),
                "expected silence after recovery, got {extra:?}"
            );
        }
    }
}
