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
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, Mutex};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use common::AgentSlot;

use crate::game::{GameResult as EngineResult, GameState};

pub mod gamehost {
    tonic::include_proto!("gamehost");
}

// Shared frame message the WatchGame stream emits and the website relays.
pub mod spectator_frame {
    tonic::include_proto!("spectator_frame");
}

use gamehost::game_host_server::{GameHost, GameHostServer};
use gamehost::{
    AgentEndpoint, AgentPlacement, GameConfig, GameResult, GameState as HostGameState, GameStatus,
    GetStatusRequest, StartGameRequest, StartGameResponse, WatchGameRequest,
};
use spectator_frame::SpectatorFrame;

/// Safety cap so a stuck game can never loop forever.
const MAX_TICKS: u64 = 100_000;
/// Upper bound on per-agent setup (`Initialize` plus opening the action
/// stream). A wedged agent fails the match fast instead of hanging it before
/// tick 0.
pub(crate) const SETUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Buffer of spectator frames a lagging subscriber can fall behind before it is
/// dropped and forced to reconnect (which re-snapshots).
const SPECTATOR_BUFFER: usize = 1024;

/// One agent's bindings for a game: match slot, engine player, platform
/// identity, and network link travel together in a single value.
///
/// Previously these lived in three parallel `Vec`s (`links`, `player_ids`,
/// `agent_ids`) coupled only by position — and `player_ids` came from
/// `HashMap` order, so slot *i* silently steered the wrong engine player
/// whenever that order differed from request order. Constructing the triple
/// once, up front, makes that class of mismatch unrepresentable downstream:
/// the loop below never indexes anything by slot.
struct Seat<L, Pid> {
    slot: AgentSlot,
    engine_pid: Pid,
    agent_id: i64,
    link: L,
}

/// The per-game seam. Owns only the typed bits: how to build the engine, how to
/// talk to this game's agents, and how the engine's `PlayerId`/`GameAction`
/// types map onto that game's agent proto.
#[async_trait::async_trait]
pub trait GameAdapter: Send + Sync + 'static {
    /// The game engine driven by this adapter.
    type Engine: GameState<PlayerId: Eq + Hash + Clone + Send + Sync, GameAction: Send>
        + Send
        + 'static;
    /// This game's typed agent gRPC client, as returned by [`Self::connect`].
    type Client: Send;
    /// Background link to one agent. The game loop publishes tick states and
    /// reads the latest action through it without ever blocking.
    type Link: Send + 'static;

    /// Accumulated spectator state used to encode snapshots and derive deltas.
    /// Holds whatever this game needs to diff frames across ticks (e.g. the
    /// trail length already sent).
    type Spectator: Send;

    /// Build a fresh engine for `num_players`. The coordinator's per-match
    /// [`GameConfig`] is passed so game-specific fields (e.g. arena size) come
    /// from a single source; the adapter falls back to its own defaults for any
    /// unset (zero) field. Slot `i` controls `get_player_ids()[i]`.
    fn init_engine(&self, num_players: usize, cfg: &GameConfig) -> Self::Engine;

    /// Seed spectator state from the freshly built engine (tick 0).
    fn init_spectator(&self, engine: &Self::Engine) -> Self::Spectator;

    /// Advance `spec` to `engine`'s current tick and return the encoded delta
    /// (game-specific proto bytes) to broadcast to spectators.
    fn tick_spectator(&self, spec: &mut Self::Spectator, engine: &Self::Engine) -> Vec<u8>;

    /// Encode the full accumulated state as snapshot bytes for a joining
    /// spectator.
    fn encode_snapshot(&self, spec: &Self::Spectator) -> Vec<u8>;

    /// Currently-alive players. Diffed across ticks to derive elimination order.
    fn active_players(&self, engine: &Self::Engine) -> Vec<<Self::Engine as GameState>::PlayerId>;

    /// Dial an agent, retrying while its VM/container finishes booting.
    async fn connect(&self, address: &str) -> Result<Self::Client, String>;

    /// One-time per-game agent setup plus link spawn: runs `Initialize`,
    /// opens the action stream, and starts the background pump tasks. Slow
    /// agents are fine after this point, but a wedge *here* fails the match,
    /// so implementations must bound it with [`SETUP_TIMEOUT`].
    ///
    /// `slot` names the agent for `Initialize.player_id` and must be the
    /// same slot carried on its [`Seat`]: it is a typed value (not a `usize`
    /// index) so setup cannot silently bind the wrong engine player.
    async fn open_link(
        &self,
        client: Self::Client,
        slot: AgentSlot,
        num_players: usize,
    ) -> Result<Self::Link, String>;

    /// Publish this tick's observation to the agent. Never blocks: states the
    /// agent hasn't drained are skipped (latest wins).
    fn push_state(&self, link: &Self::Link, tick: u64, engine: &Self::Engine);

    /// Latest action received from the agent, with the tick it was computed
    /// for. `None` if the agent hasn't answered yet: the game loop then uses
    /// [`Self::default_action`]. Stale answers apply as-is — a slow agent's
    /// intent still steers, just delayed.
    fn poll_action(
        &self,
        link: &Self::Link,
    ) -> Option<(u64, <Self::Engine as GameState>::GameAction)>;

    /// False once the agent's stream has broken (crash, disconnect). The game
    /// loop eliminates such agents; mere slowness never trips this.
    fn link_alive(&self, link: &Self::Link) -> bool;

    /// Action used when the agent has no answer yet.
    fn default_action(&self) -> <Self::Engine as GameState>::GameAction;
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

/// Accumulated spectator state, populated once the game starts. Guarded so the
/// `watch_game` handler can snapshot it and subscribe atomically with respect
/// to the per-tick producer in `run_game`.
type SpectatorState<G> = Arc<Mutex<Option<<G as GameAdapter>::Spectator>>>;

/// Generic tonic `GameHost` service, parameterised over a [`GameAdapter`].
pub struct GrpcGameServer<G: GameAdapter> {
    adapter: Arc<G>,
    progress: Progress,
    spectator: SpectatorState<G>,
    spectator_tx: broadcast::Sender<SpectatorFrame>,
}

impl<G: GameAdapter> GrpcGameServer<G> {
    pub fn new(adapter: G) -> Self {
        let (spectator_tx, _) = broadcast::channel(SPECTATOR_BUFFER);
        Self {
            adapter: Arc::new(adapter),
            progress: Arc::new(Mutex::new(GameProgress::default())),
            spectator: Arc::new(Mutex::new(None)),
            spectator_tx,
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
        let spectator = self.spectator.clone();
        let spectator_tx = self.spectator_tx.clone();
        tokio::spawn(async move {
            if let Err(e) =
                run_game(&*adapter, agents, cfg, &progress, &spectator, &spectator_tx).await
            {
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

    type WatchGameStream = Pin<Box<dyn Stream<Item = Result<SpectatorFrame, Status>> + Send>>;

    async fn watch_game(
        &self,
        _request: Request<WatchGameRequest>,
    ) -> Result<Response<Self::WatchGameStream>, Status> {
        // Snapshot the current state and subscribe under the same lock the
        // per-tick producer holds while broadcasting. This guarantees no delta
        // slips between the snapshot and the subscription: every broadcast
        // frame is either already folded into the snapshot or delivered on `rx`.
        let (snapshot, rx) = {
            let spec = self.spectator.lock().await;
            let rx = self.spectator_tx.subscribe();
            let snapshot = spec.as_ref().map(|s| SpectatorFrame {
                tick: 0,
                is_snapshot: true,
                payload: self.adapter.encode_snapshot(s),
            });
            (snapshot, rx)
        };

        // Deltas are append-only, so a dropped frame would leave a permanent
        // hole in the client's trail. When a spectator falls too far behind,
        // end the stream with an error instead: the client reconnects and
        // re-snapshots from a consistent state.
        let deltas = BroadcastStream::new(rx).map(|r| match r {
            Ok(frame) => Ok(frame),
            Err(BroadcastStreamRecvError::Lagged(n)) => Err(Status::resource_exhausted(format!(
                "spectator lagged {n} frames behind; reconnect for a fresh snapshot"
            ))),
        });
        let stream = tokio_stream::iter(snapshot.into_iter().map(Ok)).chain(deltas);

        Ok(Response::new(Box::pin(stream)))
    }
}

async fn run_game<G: GameAdapter>(
    adapter: &G,
    agents: Vec<AgentEndpoint>,
    cfg: GameConfig,
    progress: &Progress,
    spectator: &SpectatorState<G>,
    spectator_tx: &broadcast::Sender<SpectatorFrame>,
) -> Result<(), String> {
    // Resolve the wire slots into slot order, validating instead of
    // trusting Vec position: duplicates, gaps, or out-of-range slots fail
    // the match loudly here rather than mis-steering silently later.
    let num_players = agents.len();
    let mut endpoints: Vec<&AgentEndpoint> = agents.iter().collect();
    endpoints.sort_by_key(|endpoint| endpoint.slot);
    for (index, endpoint) in endpoints.iter().enumerate() {
        if endpoint.slot as usize != index {
            return Err(format!(
                "agent {} has non-contiguous slot {} for {num_players} agents",
                endpoint.agent_id, endpoint.slot
            ));
        }
    }

    let mut engine = adapter.init_engine(num_players, &cfg);
    // The tick *period*: the loop below advances on this wall-clock interval
    // no matter how fast or slow agents answer.
    let tick_period = Duration::from_millis(cfg.tick_rate_ms.max(1));

    // Seed spectator state from the initial engine so joiners can snapshot even
    // before the first tick is produced.
    *spectator.lock().await = Some(adapter.init_spectator(&engine));

    // Engine player ids arrive in init order (trait invariant on
    // `get_player_ids`), so the *i*-th id belongs to slot *i*. Zip them with
    // the validated endpoints into seats: after this point nothing is
    // addressed by bare index anymore.
    let engine_pids = engine.get_player_ids();
    if engine_pids.len() != num_players {
        return Err(format!(
            "engine created {} players for {num_players} agents",
            engine_pids.len()
        ));
    }

    // Connect every agent, then open its background link (setup + stream +
    // pump tasks). From here on the game loop never blocks on agents.
    let mut seats: Vec<Seat<G::Link, <G::Engine as GameState>::PlayerId>> =
        Vec::with_capacity(num_players);
    for ((index, endpoint), engine_pid) in endpoints.into_iter().enumerate().zip(engine_pids) {
        let slot = AgentSlot::from_index(index)
            .ok_or_else(|| format!("too many agents for slot numbering: {num_players}"))?;
        let client = adapter.connect(&endpoint.address).await?;
        let link = adapter
            .open_link(client, slot, num_players)
            .await
            .map_err(|e| format!("agent {} setup failed: {e}", endpoint.address))?;
        seats.push(Seat {
            slot,
            engine_pid,
            agent_id: endpoint.agent_id,
            link,
        });
    }

    progress.lock().await.state = HostGameState::Running;
    tracing::info!(num_players, "game running");

    let mut current_tick: u64 = 0;
    let mut alive_order = adapter.active_players(&engine);
    let mut alive_set: HashSet<_> = alive_order.iter().cloned().collect();
    let mut death_order = Vec::new();
    let mut death_tick: HashMap<_, u64> = HashMap::new();
    // Ticks whose own work (engine + spectator encoding) overran the period.
    // A rising count means the period is too tight for the engine cost, not
    // that agents are slow: agent I/O never blocks this loop.
    let mut overran_ticks: u64 = 0;

    // Fixed-rate ticker. `Skip` sheds backlog instead of spiralling: if one
    // tick overruns, the next fires on schedule rather than bursting.
    let mut interval = tokio::time::interval(tick_period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Seed tick 0 so agents have a state before the first gears turn. The
    // first interval tick fires immediately and will mostly apply defaults.
    for seat in seats.iter() {
        adapter.push_state(&seat.link, 0, &engine);
    }

    let final_result = loop {
        interval.tick().await;
        let work_start = std::time::Instant::now();

        // Consume whatever each alive agent has offered. Slow agents simply
        // steer on their latest answer or the default; only a broken stream
        // eliminates. Bindings come from the seat — never a parallel index.
        for seat in seats.iter() {
            if !alive_set.contains(&seat.engine_pid) {
                continue;
            }
            if !adapter.link_alive(&seat.link) {
                tracing::warn!(slot = seat.slot.index(), "agent link dead; dropping");
                engine.handle_player_leave(seat.engine_pid.clone());
                continue;
            }
            let action = adapter
                .poll_action(&seat.link)
                .map(|(_, action)| action)
                .unwrap_or_else(|| adapter.default_action());
            engine.handle_player_action(seat.engine_pid.clone(), action);
        }

        engine.update_game_state();
        current_tick += 1;

        // Publish this tick to spectators. Update the accumulated state and
        // broadcast the delta while holding the lock, so a concurrently-joining
        // `watch_game` either sees this delta folded into its snapshot or
        // receives it on its subscription — never neither.
        {
            let mut spec = spectator.lock().await;
            if let Some(s) = spec.as_mut() {
                let payload = adapter.tick_spectator(s, &engine);
                let _ = spectator_tx.send(SpectatorFrame {
                    tick: current_tick,
                    is_snapshot: false,
                    payload,
                });
            }
        }

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
            None => {
                // Publish the new state for the next tick, then account for
                // our own cost. Finished games break above without publishing.
                for seat in seats.iter() {
                    if alive_set.contains(&seat.engine_pid) {
                        adapter.push_state(&seat.link, current_tick, &engine);
                    }
                }
                if work_start.elapsed() > tick_period {
                    overran_ticks += 1;
                    tracing::debug!(
                        current_tick,
                        elapsed_ms = work_start.elapsed().as_millis(),
                        period_ms = tick_period.as_millis(),
                        "tick work overran its period"
                    );
                }
            }
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
            // Identity comes from the seat bound at setup — no reverse index
            // lookup that could attribute a score to the wrong agent.
            let agent_id = seats
                .iter()
                .find(|seat| seat.engine_pid == *pid)
                .map(|seat| seat.agent_id)
                .unwrap_or_default();
            AgentPlacement {
                agent_id,
                position: (idx + 1) as u32,
                // Score = ticks survived (survivors get the full game length).
                score: death_tick.get(pid).copied().unwrap_or(final_tick) as u32,
            }
        })
        .collect();

    let has_winner = matches!(final_result, EngineResult::Winner(_));
    tracing::info!(has_winner, final_tick, overran_ticks, "game finished");

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum FakeAction {
        Left,
        Right,
    }

    /// Minimal engine that records every applied action per player and ends
    /// the game after a few ticks. Player ids are plain ordinals.
    struct FakeEngine {
        num_players: u32,
        ticks: u64,
        log: Arc<StdMutex<Vec<(u32, FakeAction)>>>,
    }

    impl GameState for FakeEngine {
        type PlayerId = u32;
        type GameAction = FakeAction;
        type StateDiff = ();
        type Config = ();

        fn init_game(_config: &Self::Config, num_players: usize) -> Self {
            Self {
                num_players: num_players as u32,
                ticks: 0,
                log: Arc::new(StdMutex::new(Vec::new())),
            }
        }

        fn get_player_ids(&self) -> Vec<Self::PlayerId> {
            (0..self.num_players).collect()
        }

        fn update_game_state(&mut self) {
            self.ticks += 1;
        }

        fn handle_player_action(&mut self, player_id: Self::PlayerId, action: Self::GameAction) {
            self.log.lock().unwrap().push((player_id, action));
        }

        fn handle_player_leave(&mut self, _player_id: Self::PlayerId) {}

        fn get_game_result(&self) -> Option<EngineResult<Self::PlayerId>> {
            (self.ticks >= 3).then_some(EngineResult::Winner(0))
        }

        fn diff(&self, _other: &Self) -> Self::StateDiff {}
    }

    #[derive(Clone, Copy)]
    struct FakeLink {
        action: FakeAction,
    }

    struct FakeAdapter {
        log: Arc<StdMutex<Vec<(u32, FakeAction)>>>,
    }

    #[async_trait::async_trait]
    impl GameAdapter for FakeAdapter {
        type Engine = FakeEngine;
        type Client = ();
        type Link = FakeLink;
        type Spectator = ();

        fn init_engine(&self, num_players: usize, _cfg: &GameConfig) -> Self::Engine {
            FakeEngine {
                num_players: num_players as u32,
                ticks: 0,
                log: self.log.clone(),
            }
        }

        fn init_spectator(&self, _engine: &Self::Engine) -> Self::Spectator {}

        fn tick_spectator(&self, _spec: &mut Self::Spectator, _engine: &Self::Engine) -> Vec<u8> {
            Vec::new()
        }

        fn encode_snapshot(&self, _spec: &Self::Spectator) -> Vec<u8> {
            Vec::new()
        }

        fn active_players(&self, engine: &Self::Engine) -> Vec<u32> {
            (0..engine.num_players).collect()
        }

        async fn connect(&self, _address: &str) -> Result<Self::Client, String> {
            Ok(())
        }

        async fn open_link(
            &self,
            _client: Self::Client,
            slot: AgentSlot,
            _num_players: usize,
        ) -> Result<Self::Link, String> {
            // One constant action per slot, so a crossed wire is observable:
            // slot 0 always turns left, anything else right.
            Ok(FakeLink {
                action: if slot.index() == 0 {
                    FakeAction::Left
                } else {
                    FakeAction::Right
                },
            })
        }

        fn push_state(&self, _link: &Self::Link, _tick: u64, _engine: &Self::Engine) {}

        fn poll_action(&self, link: &Self::Link) -> Option<(u64, FakeAction)> {
            Some((0, link.action))
        }

        fn link_alive(&self, _link: &Self::Link) -> bool {
            true
        }

        fn default_action(&self) -> FakeAction {
            FakeAction::Left
        }
    }

    fn endpoint(agent_id: i64, slot: u32) -> AgentEndpoint {
        AgentEndpoint {
            agent_id,
            address: String::new(),
            slot,
        }
    }

    fn test_config() -> GameConfig {
        GameConfig {
            tick_rate_ms: 1,
            ..Default::default()
        }
    }

    /// Slot binding must follow the explicit wire slot, not Vec position:
    /// endpoints arrive shuffled, yet each engine player must still get its
    /// own slot's action and the winner's score must attribute to slot 0's
    /// agent. (The old code indexed three parallel Vecs and trusted request
    /// order on both counts.)
    #[tokio::test]
    async fn slot_binding_ignores_endpoint_order() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let adapter = FakeAdapter { log: log.clone() };
        // Deliberately shuffled: slot 1 first.
        let agents = vec![endpoint(101, 1), endpoint(100, 0)];
        let progress: Progress = Arc::new(Mutex::new(GameProgress::default()));
        let spectator: SpectatorState<FakeAdapter> = Arc::new(Mutex::new(None));
        let (tx, _) = broadcast::channel(16);
        run_game(&adapter, agents, test_config(), &progress, &spectator, &tx)
            .await
            .unwrap();

        let log = log.lock().unwrap();
        assert!(!log.is_empty(), "no actions were applied");
        for (pid, action) in log.iter() {
            let expected = if *pid == 0 {
                FakeAction::Left
            } else {
                FakeAction::Right
            };
            assert_eq!(
                *action, expected,
                "engine player {pid} got the wrong slot's action"
            );
        }

        // Winner is engine player 0 by construction; its score attributes to
        // slot 0's agent despite the shuffled request order.
        let result = progress.lock().await.result.clone().unwrap();
        assert_eq!(result.placements[0].agent_id, 100);
    }

    /// Non-contiguous wire slots fail the match loudly instead of
    /// mis-steering silently.
    #[tokio::test]
    async fn duplicate_slots_fail_fast() {
        let adapter = FakeAdapter {
            log: Arc::new(StdMutex::new(Vec::new())),
        };
        let agents = vec![endpoint(100, 0), endpoint(101, 0)];
        let progress: Progress = Arc::new(Mutex::new(GameProgress::default()));
        let spectator: SpectatorState<FakeAdapter> = Arc::new(Mutex::new(None));
        let (tx, _) = broadcast::channel(16);
        let err = run_game(&adapter, agents, test_config(), &progress, &spectator, &tx)
            .await
            .unwrap_err();
        assert!(err.contains("non-contiguous"), "unexpected error: {err}");
    }
}
