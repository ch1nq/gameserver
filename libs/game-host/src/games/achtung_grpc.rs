//! Achtung's [`GameAdapter`]: the one place Achtung's gRPC knowledge lives.
//!
//! Bridges the [`Achtung`] engine to Achtung's typed agent proto
//! (`achtung.agent`): builds the per-tick observation from `player_views()`,
//! maps the proto `Direction` reply onto [`GameAction`], and carries the arena
//! `Config` (default 1000², overridable via `ARENA_WIDTH`/`ARENA_HEIGHT`).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use prost::Message as _;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;

use std::sync::OnceLock;

use crate::game::GameState as _;
use crate::games::achtung::{Achtung, AchtungConfig, ArenaSize, BlobView, GameAction, PlayerId};
use crate::grpc::gamehost::GameConfig;
use crate::grpc::{GameAdapter, SETUP_TIMEOUT};

pub mod agentpb {
    tonic::include_proto!("achtung.agent");
}

pub mod spectpb {
    tonic::include_proto!("achtung.spectator");
}

use agentpb::agent_client::AgentClient;

/// Background link to one agent. The game loop publishes tick states through
/// `state_tx` and reads the latest action from `action_rx` without ever
/// blocking; two pump tasks shuttle messages over the `Play` stream. Latest
/// wins in both directions: states the agent hasn't drained are skipped, and
/// the freshest answered action steers, however stale.
pub struct AchtungAgentLink {
    state_tx: watch::Sender<Option<agentpb::PlayRequest>>,
    action_rx: watch::Receiver<Option<(u64, GameAction)>>,
    alive: Arc<AtomicBool>,
    /// Pump tasks, aborted on drop (links live exactly as long as the game).
    _tasks: Vec<JoinHandle<()>>,
}

impl Drop for AchtungAgentLink {
    fn drop(&mut self) {
        for task in &self._tasks {
            task.abort();
        }
    }
}

/// Marks the link dead when a pump task exits for any reason (stream
/// broke, agent disconnected, panic). Normal game-end exit also trips this,
/// which is harmless: the loop is over and nobody reads `alive` anymore.
struct LivenessGuard(Arc<AtomicBool>);

impl Drop for LivenessGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Achtung game-host adapter.
///
/// Arena size is owned by the coordinator and delivered per-match in the
/// `StartGame` [`GameConfig`]; there are no arena env vars. Since a host process
/// runs exactly one match, the resolved config is memoised in `config` on the
/// first `init_engine` so agent initialization (`open_link`) sees the very same
/// dimensions the engine was built with.
pub struct AchtungGrpc {
    /// Fallback used for any dimension the coordinator leaves unset (zero), and
    /// for standalone runs that never receive a `GameConfig`.
    default_config: AchtungConfig,
    /// Resolved config for this host's single match, set once in `init_engine`.
    config: OnceLock<AchtungConfig>,
}

impl Default for AchtungGrpc {
    fn default() -> Self {
        Self::new()
    }
}

impl AchtungGrpc {
    /// Build an adapter with default arena dimensions (1000²). Edge wrapping off.
    /// Per-match dimensions arrive later via [`GameAdapter::init_engine`].
    pub fn new() -> Self {
        Self {
            default_config: AchtungConfig {
                arena_width: 1000,
                arena_height: 1000,
                edge_wrapping: false,
            },
            config: OnceLock::new(),
        }
    }

    /// Arena config for this match: coordinator-provided dimensions, falling
    /// back to the default for any unset (zero) field.
    fn match_config(&self, cfg: &GameConfig) -> AchtungConfig {
        AchtungConfig {
            arena_width: if cfg.arena_width > 0 {
                cfg.arena_width
            } else {
                self.default_config.arena_width
            },
            arena_height: if cfg.arena_height > 0 {
                cfg.arena_height
            } else {
                self.default_config.arena_height
            },
            edge_wrapping: self.default_config.edge_wrapping,
        }
    }
}

fn map_direction(dir: i32) -> GameAction {
    match agentpb::Direction::try_from(dir) {
        Ok(agentpb::Direction::TurnLeft) => GameAction::Left,
        Ok(agentpb::Direction::TurnRight) => GameAction::Right,
        _ => GameAction::Forward,
    }
}

/// Build the agent-facing GameState snapshot for the current tick.
fn build_state(engine: &Achtung) -> agentpb::GameState {
    agentpb::GameState {
        tick: engine.tick(),
        players: engine
            .player_views()
            .into_iter()
            .map(|v| agentpb::PlayerState {
                player_id: v.player_id as u32,
                position: Some(agentpb::Position { x: v.x, y: v.y }),
                direction: v.direction,
                alive: v.alive,
            })
            .collect(),
    }
}

fn to_blob(b: &BlobView) -> spectpb::Blob {
    spectpb::Blob {
        x: b.x,
        y: b.y,
        size: b.size,
    }
}

/// One player's accumulated spectator state (mirrors the engine's append-only
/// trail so snapshots are cheap and deltas are just the newly appended blobs).
struct SpectatorPlayer {
    alive: bool,
    head: BlobView,
    body: Vec<BlobView>,
}

/// Accumulated spectator state for the whole game.
pub struct AchtungSpectator {
    tick: u64,
    arena: ArenaSize,
    players: BTreeMap<PlayerId, SpectatorPlayer>,
}

impl AchtungSpectator {
    fn from_engine(engine: &Achtung) -> Self {
        let players = engine
            .spectator_view()
            .into_iter()
            .map(|v| {
                (
                    v.player_id,
                    SpectatorPlayer {
                        alive: v.alive,
                        head: v.head,
                        body: v.body,
                    },
                )
            })
            .collect();
        Self {
            tick: engine.tick(),
            arena: engine.arena(),
            players,
        }
    }
}

#[async_trait::async_trait]
impl GameAdapter for AchtungGrpc {
    type Engine = Achtung;
    type Client = AgentClient<Channel>;
    type Link = AchtungAgentLink;
    type Spectator = AchtungSpectator;

    fn init_engine(&self, num_players: usize, cfg: &GameConfig) -> Achtung {
        // Memoise the resolved config so `open_link` initializes agents with the
        // exact arena the engine uses (a host runs a single match).
        let config = self.config.get_or_init(|| self.match_config(cfg));
        Achtung::init_game(config, num_players)
    }

    fn init_spectator(&self, engine: &Achtung) -> AchtungSpectator {
        AchtungSpectator::from_engine(engine)
    }

    fn tick_spectator(&self, spec: &mut AchtungSpectator, engine: &Achtung) -> (Vec<u8>, String) {
        spec.tick = engine.tick();
        let players = engine
            .spectator_view()
            .into_iter()
            .map(|v| {
                let acc = spec.players.entry(v.player_id).or_insert(SpectatorPlayer {
                    alive: v.alive,
                    head: v.head,
                    body: Vec::new(),
                });
                // The trail is append-only, so new blobs are whatever the engine
                // has beyond what we've already sent.
                let new_body: Vec<spectpb::Blob> = v.body[acc.body.len().min(v.body.len())..]
                    .iter()
                    .map(to_blob)
                    .collect();
                acc.body = v.body;
                acc.alive = v.alive;
                acc.head = v.head;
                spectpb::PlayerDelta {
                    player_id: v.player_id as u32,
                    alive: v.alive,
                    head: Some(to_blob(&v.head)),
                    new_body,
                }
            })
            .collect();
        let delta = spectpb::SpectatorDelta {
            tick: spec.tick,
            players,
        };
        // JSON is rendered once here; the website relay forwards it opaquely.
        // A serialization failure can only come from an in-memory struct, so
        // fall back to `{}` rather than dropping the tick.
        let json = serde_json::to_string(&delta).unwrap_or_else(|_| "{}".to_string());
        (delta.encode_to_vec(), json)
    }

    fn encode_snapshot(&self, spec: &AchtungSpectator) -> (Vec<u8>, String) {
        let players = spec
            .players
            .iter()
            .map(|(&player_id, p)| spectpb::PlayerBody {
                player_id: player_id as u32,
                alive: p.alive,
                head: Some(to_blob(&p.head)),
                body: p.body.iter().map(to_blob).collect(),
            })
            .collect();
        let snapshot = spectpb::SpectatorSnapshot {
            tick: spec.tick,
            arena: Some(spectpb::ArenaConfig {
                width: spec.arena.width,
                height: spec.arena.height,
            }),
            players,
        };
        let json = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string());
        (snapshot.encode_to_vec(), json)
    }

    fn active_players(&self, engine: &Achtung) -> Vec<PlayerId> {
        engine
            .player_views()
            .into_iter()
            .filter(|v| v.alive)
            .map(|v| v.player_id)
            .collect()
    }

    async fn connect(&self, address: &str) -> Result<Self::Client, String> {
        let url = format!("http://{address}");
        let mut last = String::new();
        for _ in 0..30 {
            match AgentClient::connect(url.clone()).await {
                Ok(client) => return Ok(client),
                Err(e) => {
                    last = e.to_string();
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        Err(format!("could not connect to agent {address}: {last}"))
    }

    async fn open_link(
        &self,
        mut client: Self::Client,
        player_slot: usize,
        num_players: usize,
    ) -> Result<Self::Link, String> {
        let config = self.config.get().unwrap_or(&self.default_config);
        let arena = Some(agentpb::ArenaConfig {
            width: config.arena_width,
            height: config.arena_height,
        });
        tokio::time::timeout(
            SETUP_TIMEOUT,
            client.initialize(agentpb::InitializeRequest {
                player_id: player_slot as u32,
                num_players: num_players as u32,
                arena,
            }),
        )
        .await
        .map_err(|_| format!("agent Initialize timed out after {SETUP_TIMEOUT:?}"))?
        .map(|_| ())
        .map_err(|e| e.to_string())?;

        // Depth 1 suffices: at most one unsent state exists, and `send` only
        // blocks while the transport hasn't drained it.
        let (stream_tx, stream_rx) = mpsc::channel(1);
        let mut inbound =
            tokio::time::timeout(SETUP_TIMEOUT, client.play(ReceiverStream::new(stream_rx)))
                .await
                .map_err(|_| format!("agent Play open timed out after {SETUP_TIMEOUT:?}"))?
                .map_err(|e| format!("agent Play open failed ({e}); rebuild the agent image"))?
                .into_inner();
        tracing::info!("agent Play stream open");

        let (state_tx, mut state_rx) = watch::channel(None);
        let (action_tx, action_rx) = watch::channel(None);
        let alive = Arc::new(AtomicBool::new(true));

        // Forwards the newest published state; blocks on backpressure without
        // affecting the game loop or the receiver task below.
        let send_task = {
            let alive = alive.clone();
            tokio::spawn(async move {
                let _liveness = LivenessGuard(alive);
                while state_rx.changed().await.is_ok() {
                    let Some(req) = state_rx.borrow_and_update().clone() else {
                        continue;
                    };
                    if stream_tx.send(req).await.is_err() {
                        break;
                    }
                }
            })
        };
        // Publishes every answered action, latest wins. Ends (marking the
        // link dead) when the agent closes or breaks the stream.
        let recv_task = {
            let alive = alive.clone();
            tokio::spawn(async move {
                let _liveness = LivenessGuard(alive);
                loop {
                    match inbound.message().await {
                        Ok(Some(resp)) => {
                            let action =
                                map_direction(resp.action.map(|a| a.direction).unwrap_or_default());
                            action_tx.send_replace(Some((resp.tick, action)));
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            })
        };

        Ok(AchtungAgentLink {
            state_tx,
            action_rx,
            alive,
            _tasks: vec![send_task, recv_task],
        })
    }

    fn push_state(&self, link: &Self::Link, tick: u64, engine: &Achtung, _player_slot: usize) {
        let state = build_state(engine);
        link.state_tx.send_replace(Some(agentpb::PlayRequest {
            tick,
            state: Some(state),
        }));
    }

    fn poll_action(&self, link: &Self::Link) -> Option<(u64, GameAction)> {
        *link.action_rx.borrow()
    }

    fn link_alive(&self, link: &Self::Link) -> bool {
        link.alive.load(Ordering::SeqCst)
    }

    fn default_action(&self) -> GameAction {
        GameAction::Forward
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::GameAdapter;

    use agentpb::agent_server::{Agent, AgentServer};
    use agentpb::{
        AgentAction, Direction, InitializeRequest, InitializeResponse, PlayRequest, PlayResponse,
    };
    use tokio::net::TcpListener;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::Server;
    use tonic::{Request, Response, Status, Streaming};

    /// Test agent: answers every `Play` request with `TurnLeft` after an
    /// optional delay, until `die` fires (clean stream close).
    struct TestAgent {
        delay: Duration,
        die: Arc<tokio::sync::Notify>,
    }

    #[tonic::async_trait]
    impl Agent for TestAgent {
        async fn initialize(
            &self,
            _request: Request<InitializeRequest>,
        ) -> Result<Response<InitializeResponse>, Status> {
            Ok(Response::new(InitializeResponse {}))
        }

        type PlayStream = ReceiverStream<Result<PlayResponse, Status>>;

        async fn play(
            &self,
            request: Request<Streaming<PlayRequest>>,
        ) -> Result<Response<Self::PlayStream>, Status> {
            let mut inbound = request.into_inner();
            let (tx, rx) = mpsc::channel(16);
            let delay = self.delay;
            let die = self.die.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = die.notified() => break,
                        msg = inbound.message() => {
                            let Ok(Some(req)) = msg else { break };
                            if !delay.is_zero() {
                                tokio::time::sleep(delay).await;
                            }
                            let resp = PlayResponse {
                                tick: req.tick,
                                action: Some(AgentAction {
                                    direction: Direction::TurnLeft as i32,
                                }),
                            };
                            if tx.send(Ok(resp)).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }

    fn adapter() -> AchtungGrpc {
        AchtungGrpc {
            default_config: AchtungConfig::default(),
            config: OnceLock::new(),
        }
    }

    fn engine() -> Achtung {
        Achtung::init_game(&AchtungConfig::default(), 1)
    }

    async fn spawn_agent(
        delay: Duration,
    ) -> (
        String,
        Arc<tokio::sync::Notify>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let die = Arc::new(tokio::sync::Notify::new());
        let agent = TestAgent {
            delay,
            die: die.clone(),
        };
        let handle = tokio::spawn(async move {
            Server::builder()
                .add_service(AgentServer::new(agent))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        // Let the listener settle before dialling.
        tokio::time::sleep(Duration::from_millis(50)).await;
        (addr, die, handle)
    }

    #[tokio::test]
    async fn link_roundtrips_latest_action() {
        let adapter = adapter();
        let (addr, _die, server) = spawn_agent(Duration::ZERO).await;
        let client = adapter.connect(&addr).await.unwrap();
        let link = adapter.open_link(client, 0, 1).await.unwrap();
        let engine = engine();

        adapter.push_state(&link, 5, &engine, 0);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let action = loop {
            if let Some((tick, action)) = adapter.poll_action(&link) {
                break (tick, action);
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "no action arrived within 5s"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        assert_eq!(action, (5, GameAction::Left));
        assert!(adapter.link_alive(&link));
        server.abort();
    }

    #[tokio::test]
    async fn unanswered_link_polls_none_and_defaults_forward() {
        let adapter = adapter();
        // Replies take an hour: effectively never within the test.
        let (addr, _die, server) = spawn_agent(Duration::from_secs(3600)).await;
        let client = adapter.connect(&addr).await.unwrap();
        let link = adapter.open_link(client, 0, 1).await.unwrap();

        adapter.push_state(&link, 1, &engine(), 0);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(adapter.poll_action(&link).is_none());
        assert_eq!(adapter.default_action(), GameAction::Forward);
        // Slow does not mean dead.
        assert!(adapter.link_alive(&link));
        server.abort();
    }

    #[tokio::test]
    async fn killed_agent_marks_link_dead() {
        let adapter = adapter();
        let (addr, die, _server) = spawn_agent(Duration::ZERO).await;
        let client = adapter.connect(&addr).await.unwrap();
        let link = adapter.open_link(client, 0, 1).await.unwrap();
        assert!(adapter.link_alive(&link));

        // Clean stream close from the agent side.
        die.notify_waiters();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if !adapter.link_alive(&link) {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "link still alive 5s after server death"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
