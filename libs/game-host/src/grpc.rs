//! Generic gRPC game host.
//!
//! [`GrpcGameServer`] implements the generic `gamehost` control contract the
//! coordinator drives (`StartGame` / `GetStatus`) and owns *all* orchestration:
//! the session map, the per-tick loop, elimination tracking and placement
//! ordering. Everything game-specific lives behind the [`GameAdapter`] seam — a
//! per-game adapter bridges the engine ([`crate::game::GameState`]) and that
//! game's own typed agent proto. Adding a game means writing one adapter; the
//! coordinator and this file never change.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::game::{GameResult as EngineResult, GameState};

pub mod gamehost {
    tonic::include_proto!("gamehost");
}

use gamehost::game_host_server::{GameHost, GameHostServer};
use gamehost::{
    AgentEndpoint, AgentPlacement, GameConfig, GameResult, GameState as HostGameState, GameStatus,
    GetStatusRequest, StartGameRequest, StartGameResponse,
};

/// Safety cap so a stuck game can never loop forever.
const MAX_TICKS: u64 = 100_000;

/// The per-game seam. Owns only the typed bits: how to build the engine, how to
/// talk to this game's agents, and how the engine's `PlayerId`/`GameAction`
/// types map onto that game's agent proto.
#[async_trait::async_trait]
pub trait GameAdapter: Send + Sync + 'static {
    /// The game engine driven by this adapter.
    type Engine: GameState<PlayerId: Eq + Hash + Clone + Send + Sync, GameAction: Send>
        + Send
        + 'static;
    /// This game's typed agent gRPC client.
    type Client: Send;

    /// Build a fresh engine for `num_players`. The adapter owns the game
    /// `Config` (arena size, etc.); slot `i` controls `get_player_ids()[i]`.
    fn init_engine(&self, num_players: usize) -> Self::Engine;

    /// Currently-alive players. Diffed across ticks to derive elimination order.
    fn active_players(&self, engine: &Self::Engine) -> Vec<<Self::Engine as GameState>::PlayerId>;

    /// Dial an agent, retrying while its VM/container finishes booting.
    async fn connect(&self, address: &str) -> Result<Self::Client, String>;

    /// One-time per-game agent setup (proto `Initialize`).
    async fn initialize(
        &self,
        client: &mut Self::Client,
        player_slot: usize,
        num_players: usize,
    ) -> Result<(), String>;

    /// Build this tick's typed observation for `player_slot`, call the agent's
    /// `GetAction`, and map the reply into an engine action.
    async fn get_action(
        &self,
        client: &mut Self::Client,
        engine: &Self::Engine,
        player_slot: usize,
    ) -> Result<<Self::Engine as GameState>::GameAction, String>;
}

/// Progress of the single game this host runs. A game-host process hosts
/// exactly one game (the coordinator spawns a fresh VM per match and destroys
/// it afterwards), so there is no session map — just this one cell.
struct GameProgress {
    started: bool,
    state: HostGameState,
    current_tick: u64,
    result: Option<GameResult>,
}

impl Default for GameProgress {
    fn default() -> Self {
        Self {
            started: false,
            state: HostGameState::Unspecified,
            current_tick: 0,
            result: None,
        }
    }
}

type Progress = Arc<Mutex<GameProgress>>;

/// Generic tonic `GameHost` service, parameterised over a [`GameAdapter`].
pub struct GrpcGameServer<G: GameAdapter> {
    adapter: Arc<G>,
    progress: Progress,
}

impl<G: GameAdapter> GrpcGameServer<G> {
    pub fn new(adapter: G) -> Self {
        Self {
            adapter: Arc::new(adapter),
            progress: Arc::new(Mutex::new(GameProgress::default())),
        }
    }

    /// Serve the `GameHost` gRPC service on `0.0.0.0:port` until shutdown.
    pub async fn serve(self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("0.0.0.0:{port}").parse()?;
        tracing::info!(%addr, "game host listening");
        Server::builder()
            .add_service(GameHostServer::new(self))
            .serve(addr)
            .await?;
        Ok(())
    }
}

#[tonic::async_trait]
impl<G: GameAdapter> GameHost for GrpcGameServer<G> {
    async fn start_game(
        &self,
        request: Request<StartGameRequest>,
    ) -> Result<Response<StartGameResponse>, Status> {
        let req = request.into_inner();
        let cfg = req.config.unwrap_or_default();
        let agents = req.agents;
        if agents.is_empty() {
            return Err(Status::invalid_argument("no agents provided"));
        }

        {
            let mut progress = self.progress.lock().await;
            if progress.started {
                return Err(Status::failed_precondition("game already started"));
            }
            progress.started = true;
            progress.state = HostGameState::WaitingForAgents;
        }

        let progress = self.progress.clone();
        let adapter = self.adapter.clone();
        tokio::spawn(async move {
            if let Err(e) = run_game(&*adapter, agents, cfg, &progress).await {
                tracing::error!(error = %e, "game failed");
                let mut p = progress.lock().await;
                p.state = HostGameState::Failed;
                p.result = Some(GameResult {
                    placements: vec![],
                    error: e,
                });
            }
        });

        Ok(Response::new(StartGameResponse {}))
    }

    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GameStatus>, Status> {
        let p = self.progress.lock().await;
        Ok(Response::new(GameStatus {
            state: p.state as i32,
            result: p.result.clone(),
            current_tick: p.current_tick,
        }))
    }
}

async fn run_game<G: GameAdapter>(
    adapter: &G,
    agents: Vec<AgentEndpoint>,
    cfg: GameConfig,
    progress: &Progress,
) -> Result<(), String> {
    let num_players = agents.len();
    let tick_rate = Duration::from_millis(cfg.tick_rate_ms.max(1));

    let mut engine = adapter.init_engine(num_players);

    // Stable slot -> engine player id mapping: slot i is controlled by agents[i].
    let player_ids = engine.get_player_ids();
    if player_ids.len() != num_players {
        return Err(format!(
            "engine created {} players for {num_players} agents",
            player_ids.len()
        ));
    }
    let agent_ids: Vec<i64> = agents.iter().map(|a| a.agent_id).collect();

    // Connect + initialize every agent.
    let mut clients: Vec<G::Client> = Vec::with_capacity(num_players);
    for (slot, endpoint) in agents.iter().enumerate() {
        let mut client = adapter.connect(&endpoint.address).await?;
        adapter
            .initialize(&mut client, slot, num_players)
            .await
            .map_err(|e| format!("agent {} initialize failed: {e}", endpoint.address))?;
        clients.push(client);
    }

    progress.lock().await.state = HostGameState::Running;
    tracing::info!(num_players, "game running");

    let mut current_tick: u64 = 0;
    let mut alive_order = adapter.active_players(&engine);
    let mut alive_set: HashSet<_> = alive_order.iter().cloned().collect();
    let mut death_order = Vec::new();
    let mut death_tick: HashMap<_, u64> = HashMap::new();

    let final_result = loop {
        // Ask each still-alive agent for its action for this tick.
        for (slot, client) in clients.iter_mut().enumerate() {
            let pid = &player_ids[slot];
            if !alive_set.contains(pid) {
                continue;
            }
            match adapter.get_action(client, &engine, slot).await {
                Ok(action) => engine.handle_player_action(pid.clone(), action),
                Err(e) => {
                    tracing::warn!(slot, error = %e, "agent action failed; dropping");
                    engine.handle_player_leave(pid.clone());
                }
            }
        }

        engine.update_game_state();
        current_tick += 1;

        // Record any newly-dead players (previous-alive order → placement order).
        let new_alive = adapter.active_players(&engine);
        let new_set: HashSet<_> = new_alive.iter().cloned().collect();
        for pid in &alive_order {
            if !new_set.contains(pid) {
                death_tick.insert(pid.clone(), current_tick);
                death_order.push(pid.clone());
            }
        }
        alive_order = new_alive;
        alive_set = new_set;

        progress.lock().await.current_tick = current_tick;

        match engine.get_game_result() {
            Some(r) => break r,
            None if current_tick >= MAX_TICKS => break EngineResult::NoWinner,
            None => tokio::time::sleep(tick_rate).await,
        }
    };

    // Final ranking: survivor(s) first, then most-recently-dead → earliest.
    let final_tick = current_tick;
    let mut ranking = alive_order;
    ranking.extend(death_order.iter().rev().cloned());

    let placements: Vec<AgentPlacement> = ranking
        .iter()
        .enumerate()
        .map(|(idx, pid)| {
            let slot = player_ids.iter().position(|p| p == pid);
            AgentPlacement {
                agent_id: slot
                    .and_then(|s| agent_ids.get(s).copied())
                    .unwrap_or_default(),
                position: (idx + 1) as u32,
                // Score = ticks survived (survivors get the full game length).
                score: death_tick.get(pid).copied().unwrap_or(final_tick) as u32,
            }
        })
        .collect();

    let has_winner = matches!(final_result, EngineResult::Winner(_));
    tracing::info!(has_winner, final_tick, "game finished");

    {
        let mut p = progress.lock().await;
        p.state = HostGameState::Finished;
        p.current_tick = final_tick;
        p.result = Some(GameResult {
            placements,
            error: String::new(),
        });
    }

    Ok(())
}
