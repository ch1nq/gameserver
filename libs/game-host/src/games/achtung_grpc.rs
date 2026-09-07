//! Achtung's [`GameAdapter`]: the one place Achtung's gRPC knowledge lives.
//!
//! Bridges the [`Achtung`] engine to Achtung's typed agent proto
//! (`achtung.agent`): builds the per-tick observation from `player_views()`,
//! maps the proto `Direction` reply onto [`GameAction`], and carries the arena
//! `Config` (default 1000², overridable via `ARENA_WIDTH`/`ARENA_HEIGHT`).

use std::collections::BTreeMap;
use std::time::Duration;

use prost::Message as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::{Code, Streaming};

use crate::game::GameState as _;
use crate::games::achtung::{Achtung, AchtungConfig, ArenaSize, BlobView, GameAction, PlayerId};
use crate::grpc::GameAdapter;

pub mod agentpb {
    tonic::include_proto!("achtung.agent");
}

pub mod spectpb {
    tonic::include_proto!("achtung.spectator");
}

use agentpb::agent_client::AgentClient;

/// Live `Play` stream halves for one agent. Lockstep by construction: the host
/// sends one request and reads one reply per tick, so at most one message is
/// ever in flight in either direction and no tick can be answered stale.
struct PlayStream {
    tx: mpsc::Sender<agentpb::PlayRequest>,
    rx: Streaming<agentpb::PlayResponse>,
}

/// Connection to one agent. Prefers the long-lived `Play` stream (sub-ms per
/// tick on an open stream through the microVM relay, versus ~44ms per unary
/// call); falls back to unary `GetAction` when the agent does not implement
/// `Play`, or if the stream dies mid-game.
pub struct AchtungAgentClient {
    client: AgentClient<Channel>,
    play: Option<PlayStream>,
}

impl AchtungAgentClient {
    /// Open the per-game `Play` stream. `None` means unary mode: either the
    /// agent predates `Play` (`UNIMPLEMENTED`) or the open failed, in which
    /// case the following `Initialize`/action calls surface the real error
    /// exactly as they did before streaming existed.
    async fn open_play(client: &mut AgentClient<Channel>, address: &str) -> Option<PlayStream> {
        // Depth 1 suffices: lockstep means at most one unsent request exists,
        // and `send` only blocks while the transport hasn't drained it.
        let (tx, rx) = mpsc::channel(1);
        match client.play(ReceiverStream::new(rx)).await {
            Ok(resp) => {
                tracing::info!(address, "agent uses Play stream");
                Some(PlayStream {
                    tx,
                    rx: resp.into_inner(),
                })
            }
            Err(e) if e.code() == Code::Unimplemented => {
                tracing::info!(address, "agent predates Play; using unary GetAction");
                None
            }
            Err(e) => {
                tracing::warn!(address, error = %e, "Play open failed; trying unary");
                None
            }
        }
    }

    /// One lockstep exchange on the open stream.
    async fn play_action(
        &mut self,
        tick: u64,
        state: agentpb::GameState,
    ) -> Result<GameAction, String> {
        let stream = self.play.as_mut().ok_or("no Play stream")?;
        stream
            .tx
            .send(agentpb::PlayRequest {
                tick,
                state: Some(state),
            })
            .await
            .map_err(|e| format!("Play send failed: {e}"))?;
        let resp = stream
            .rx
            .message()
            .await
            .map_err(|e| format!("Play recv failed: {e}"))?
            .ok_or("Play stream closed by agent")?;
        if resp.tick != tick {
            return Err(format!(
                "stale Play response: got tick {}, want {tick}",
                resp.tick
            ));
        }
        Ok(map_direction(
            resp.action.map(|a| a.direction).unwrap_or_default(),
        ))
    }
}

/// Achtung game-host adapter. Holds the arena configuration used to build the
/// engine and initialize agents.
pub struct AchtungGrpc {
    config: AchtungConfig,
}

impl AchtungGrpc {
    /// Build an adapter, reading arena dimensions from `ARENA_WIDTH` /
    /// `ARENA_HEIGHT` (default 1000² if unset/unparseable). Edge wrapping off.
    pub fn from_env() -> Self {
        let dim = |key: &str| {
            std::env::var(key)
                .ok()
                .and_then(|s| s.parse().ok())
                .filter(|&v| v > 0)
                .unwrap_or(1000)
        };
        Self {
            config: AchtungConfig {
                arena_width: dim("ARENA_WIDTH"),
                arena_height: dim("ARENA_HEIGHT"),
                edge_wrapping: false,
            },
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
    type Client = AchtungAgentClient;
    type Spectator = AchtungSpectator;

    fn init_engine(&self, num_players: usize) -> Achtung {
        Achtung::init_game(&self.config, num_players)
    }

    fn init_spectator(&self, engine: &Achtung) -> AchtungSpectator {
        AchtungSpectator::from_engine(engine)
    }

    fn tick_spectator(&self, spec: &mut AchtungSpectator, engine: &Achtung) -> Vec<u8> {
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
        spectpb::SpectatorDelta {
            tick: spec.tick,
            players,
        }
        .encode_to_vec()
    }

    fn encode_snapshot(&self, spec: &AchtungSpectator) -> Vec<u8> {
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
        spectpb::SpectatorSnapshot {
            tick: spec.tick,
            arena: Some(spectpb::ArenaConfig {
                width: spec.arena.width,
                height: spec.arena.height,
            }),
            players,
        }
        .encode_to_vec()
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
                Ok(mut client) => {
                    let play = AchtungAgentClient::open_play(&mut client, address).await;
                    return Ok(AchtungAgentClient { client, play });
                }
                Err(e) => {
                    last = e.to_string();
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        Err(format!("could not connect to agent {address}: {last}"))
    }

    async fn initialize(
        &self,
        client: &mut Self::Client,
        player_slot: usize,
        num_players: usize,
    ) -> Result<(), String> {
        client
            .client
            .initialize(agentpb::InitializeRequest {
                player_id: player_slot as u32,
                num_players: num_players as u32,
                arena: Some(agentpb::ArenaConfig {
                    width: self.config.arena_width,
                    height: self.config.arena_height,
                }),
            })
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn get_action(
        &self,
        client: &mut Self::Client,
        engine: &Achtung,
        _player_slot: usize,
    ) -> Result<GameAction, String> {
        let tick = engine.tick();
        let state = build_state(engine);
        if client.play.is_some() {
            match client.play_action(tick, state.clone()).await {
                ok @ Ok(_) => return ok,
                Err(e) => {
                    // The stream died mid-game: drop back to unary for the
                    // rest of the match rather than failing every tick.
                    tracing::warn!(error = %e, "Play stream failed; falling back to unary");
                    client.play = None;
                }
            }
        }
        client
            .client
            .get_action(state)
            .await
            .map(|resp| map_direction(resp.into_inner().direction))
            .map_err(|e| e.to_string())
    }
}
