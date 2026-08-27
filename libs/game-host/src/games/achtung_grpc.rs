//! Achtung's [`GameAdapter`]: the one place Achtung's gRPC knowledge lives.
//!
//! Bridges the [`Achtung`] engine to Achtung's typed agent proto
//! (`achtung.agent`): builds the per-tick observation from `player_views()`,
//! maps the proto `Direction` reply onto [`GameAction`], and carries the arena
//! `Config` (default 1000², overridable via `ARENA_WIDTH`/`ARENA_HEIGHT`).

use std::time::Duration;

use tonic::transport::Channel;

use crate::game::GameState as _;
use crate::games::achtung::{Achtung, AchtungConfig, GameAction, PlayerId};
use crate::grpc::GameAdapter;

pub mod agentpb {
    tonic::include_proto!("achtung.agent");
}

use agentpb::agent_client::AgentClient;

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

#[async_trait::async_trait]
impl GameAdapter for AchtungGrpc {
    type Engine = Achtung;
    type Client = AgentClient<Channel>;

    fn init_engine(&self, num_players: usize) -> Achtung {
        Achtung::init_game(&self.config, num_players)
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
                Ok(c) => return Ok(c),
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
        let state = build_state(engine);
        client
            .get_action(state)
            .await
            .map(|resp| map_direction(resp.into_inner().direction))
            .map_err(|e| e.to_string())
    }
}
