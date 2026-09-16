use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use agent_infra::{
    AgentSlot, AgentSpawnConfig, ContainerImage, HostSpawnConfig, MachineError, MachineHandle,
    MachineProvider, MatchLayout,
};
use common::{AgentId, AgentInfo, AgentRepository, ContainerImageUrl, DeployTokenProvider};
use common::{FinishedPlacement, MatchRecorder, StoredRating};
use game_host::game_host_client::GameHostClient;
use game_host::{AgentEndpoint, GameConfig, GameState, GetStatusRequest, StartGameRequest};
use ranking::{FfaPlayer, Rank, ValidatedFfaPlayers, WengLinConfig, WengLinRating};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

// Re-export types for public API
pub use common::ImageUrl;

// Generated from protos/game_host.proto
pub mod game_host {
    tonic::include_proto!("gamehost");
}

// Shared frame message produced by the game host. The browser-facing SSE handler
// (in the website) reuses this to decode the WatchGame stream it relays.
pub mod spectator_frame {
    tonic::include_proto!("spectator_frame");
}

/// The gRPC URL of a game host. Newtyped so registry readers can't confuse it
/// with an arbitrary string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameHostAddr(String);

impl GameHostAddr {
    pub fn new(addr: impl Into<String>) -> Self {
        Self(addr.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GameHostAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One player slot in the current match, in `StartGame` order (slot `i` is
/// controlled by `agents[i]` and renders with `PLAYER_COLORS[i]` in the browser).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LineupEntry {
    pub slot: usize,
    pub agent_id: AgentId,
    pub name: String,
}

/// Terminal result of a match, published for spectators. Mirrors the
/// `GameResult` proto but is `Clone + Serialize` for SSE fan-out.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpectatorResult {
    pub placements: Vec<AgentPlacement>,
    pub error: String,
}

/// What the coordinator shares with the website's spectator relay. One game
/// runs at a time, so a single slot suffices. `addr` is `Some` while a game
/// host is live; `lineup` describes the current (or most recent) match and
/// `last_result` retains the terminal result so late joiners and the relay's
/// game-over path can display it after the host is destroyed.
#[derive(Debug, Clone, Default)]
pub struct SpectatorMatch {
    pub addr: Option<GameHostAddr>,
    pub lineup: Vec<LineupEntry>,
    pub last_result: Option<SpectatorResult>,
}

/// Shared between the coordinator (writer) and the spectator relay (reader).
pub type SpectatorRegistry = Arc<RwLock<SpectatorMatch>>;

/// Publishes a game host + lineup to the [`SpectatorRegistry`] for its lifetime
/// and clears the address on drop, so a cancelled or panicking match never
/// leaves a stale address pointing at a destroyed host. Clearing an explicit
/// `None` at the end of the happy path is not enough: match tasks can be
/// cancelled (e.g. a match timeout) between publish and clear. The lineup and
/// last result are retained so spectators still see who played and who won.
struct SpectatorRegistryGuard {
    registry: SpectatorRegistry,
    addr: GameHostAddr,
}

impl SpectatorRegistryGuard {
    /// Publish `addr` + `lineup` (clearing any stale result) and return a guard
    /// that clears the address on drop.
    async fn publish(
        registry: SpectatorRegistry,
        addr: GameHostAddr,
        lineup: Vec<LineupEntry>,
    ) -> Self {
        {
            let mut slot = registry.write().await;
            slot.addr = Some(addr.clone());
            slot.lineup = lineup;
            slot.last_result = None;
        }
        Self { registry, addr }
    }

    /// Record the terminal result so the relay can emit a `result` SSE event
    /// even after the game host is destroyed.
    async fn set_result(&self, result: SpectatorResult) {
        self.registry.write().await.last_result = Some(result);
    }
}

impl Drop for SpectatorRegistryGuard {
    fn drop(&mut self) {
        // Drop can't await, so hand the clear to the runtime. Only clear if we
        // are still the published address, so a newer game isn't clobbered.
        // Lineup + result are kept for the between-games overlay.
        let registry = self.registry.clone();
        let addr = std::mem::take(&mut self.addr);
        tokio::spawn(async move {
            let mut slot = registry.write().await;
            if slot.addr.as_ref() == Some(&addr) {
                slot.addr = None;
            }
        });
    }
}

/// Configuration for the game coordinator
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Image URL for the game host container
    ///
    /// Points to a public registry image (e.g., ghcr.io/ch1nq/achtung-game-host:latest)
    /// that is used directly without copying through the local registry.
    /// User agent images continue to use the local registry workflow.
    pub game_host_image: ImageUrl,

    /// Number of agents per game
    pub agents_per_game: usize,

    /// Game tick rate in milliseconds
    pub tick_rate_ms: u64,

    /// Arena width, passed to the game host in the `StartGame` config.
    pub arena_width: u32,

    /// Arena height, passed to the game host in the `StartGame` config.
    pub arena_height: u32,

    /// How long to wait between games
    pub game_interval: Duration,

    /// How often to poll game status
    pub poll_interval: Duration,

    /// gRPC port that the game host listens on *inside* its machine.
    ///
    /// Also the fallback dial port when a backend does not relay through a
    /// published host port (see [`MachineHandle::grpc_port`]).
    pub game_host_grpc_port: u16,

    /// gRPC port that agents listen on *inside* their machines. Same fallback
    /// role as [`Self::game_host_grpc_port`].
    pub agent_grpc_port: u16,

    /// How long to keep retrying the initial connection to a freshly spawned
    /// game host before giving up.
    ///
    /// Boot time varies by orders of magnitude across backends — a container
    /// start versus a microVM boot plus an image pull — so this is a deadline on
    /// a retry loop rather than a fixed sleep. A single guessed sleep is either
    /// too short (spurious failures) or too slow (wasted on every match).
    pub game_host_connect_timeout: Duration,

    /// Weng-Lin rating parameters (beta = skill-class width). Defaults to
    /// [`default_weng_lin_config`]; surfaced here so tests and future
    /// config can tune it without touching the rating call sites.
    pub weng_lin_config: WengLinConfig,
}

/// Default [`WengLinConfig`] for Elo-scale ratings (beta = 250, matching
/// the ×60 rescale). Re-exported so the website can build its
/// `CoordinatorConfig` without depending on `achtung-ranking` directly.
/// Never use `WengLinConfig::new()` here — its raw beta would saturate
/// every win probability at this scale and cause wild rating swings.
pub fn default_weng_lin_config() -> WengLinConfig {
    ranking::default_config()
}

/// The game coordinator that orchestrates matches.
///
/// Generic over the [`MachineProvider`] rather than boxed, because the provider
/// carries an associated `MatchContext` that flows through the match lifecycle.
/// Held as `Arc<P>` so the reaper can share the same provider instance.
pub struct GameCoordinator<P: MachineProvider> {
    config: CoordinatorConfig,
    machine_provider: Arc<P>,
    agent_repo: Box<dyn AgentRepository>,
    token_provider: Box<dyn DeployTokenProvider>,
    match_recorder: Box<dyn MatchRecorder>,
    /// Publishes the current game host address for the spectator relay.
    spectator_registry: SpectatorRegistry,
}

impl<P: MachineProvider> GameCoordinator<P> {
    pub fn new(
        config: CoordinatorConfig,
        machine_provider: Arc<P>,
        agent_repo: Box<dyn AgentRepository>,
        token_provider: Box<dyn DeployTokenProvider>,
        match_recorder: Box<dyn MatchRecorder>,
        spectator_registry: SpectatorRegistry,
    ) -> Self {
        Self {
            config,
            machine_provider,
            agent_repo,
            token_provider,
            match_recorder,
            spectator_registry,
        }
    }

    /// Run a single game to completion (used for testing / one-shot runs).
    pub async fn run_once(&self) -> Result<(), CoordinatorError> {
        self.run_single_game().await
    }

    /// Spawn the coordinator as a background task
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Main coordinator loop
    async fn run(self) {
        tracing::info!("Game coordinator started");

        loop {
            match self.run_single_game().await {
                Ok(()) => {
                    tracing::info!("Game completed successfully");
                }
                Err(e) => {
                    tracing::error!("Game failed: {}", e);
                }
            }

            tokio::time::sleep(self.config.game_interval).await;
        }
    }

    /// Run a single game from start to finish
    async fn run_single_game(&self) -> Result<(), CoordinatorError> {
        // 1. Pick agents from the roster
        let agents = self
            .agent_repo
            .get_random_active_agents(self.config.agents_per_game)
            .await
            .map_err(CoordinatorError::Database)?;

        if agents.len() < self.config.agents_per_game {
            tracing::warn!(
                "Not enough active agents ({}/{}), skipping game",
                agents.len(),
                self.config.agents_per_game
            );
            return Ok(());
        }

        tracing::info!("Starting game with {} agents", agents.len());

        // 2. Initialize match infrastructure (network, etc.)
        let match_id = agent_infra::generate_id();
        // Validated once: rejects zero agents and counts that would overflow
        // the u8 wire slot or the relay port range.
        let layout = MatchLayout::new(agents.len()).map_err(CoordinatorError::MachineSpawn)?;
        let ctx = self
            .machine_provider
            .init_match(&match_id, layout)
            .await
            .map_err(CoordinatorError::MachineSpawn)?;

        // 3. Run the game, then always clean up
        let game_result = self.run_game_inner(&ctx, layout, &agents).await;

        // 4. Cleanup match infrastructure regardless of outcome
        if let Err(e) = self.machine_provider.cleanup_match(ctx).await {
            tracing::error!("Failed to cleanup match {}: {}", match_id, e);
        }

        match game_result {
            Ok(result) => {
                tracing::info!("Game finished: {:?}", result);
                // Ratings are best-effort: a persistence failure is logged but
                // doesn't fail the match loop (infra is already cleaned up).
                self.update_ratings(&match_id, &result).await;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Load current ratings, compute Weng-Lin updates for a `Finished` game,
    /// and persist them. Failed games never reach here, so every placement
    /// moves ratings. Ties share a `position` and are scored as tied.
    ///
    /// Best-effort with bounded retries: rating I/O is retried a few times
    /// with backoff, but a persistent failure only logs — it never fails the
    /// match loop (infra is already cleaned up). A skipped match leaves a
    /// gap in history by design; see the retry constants below.
    async fn update_ratings(&self, match_id: &str, result: &GameResult) {
        if result.placements.is_empty() {
            return;
        }
        // Fail fast before any I/O: duplicates would otherwise rate one
        // agent twice and die opaquely on the DB primary key.
        {
            let mut seen = HashSet::with_capacity(result.placements.len());
            for p in &result.placements {
                if !seen.insert(p.agent_id) {
                    tracing::warn!(
                        match_id,
                        agent_id = p.agent_id,
                        "Skipping rating update: duplicate agent in placements"
                    );
                    return;
                }
            }
        }
        // Parse wire positions into `Rank` (non-zero by construction) before
        // any I/O: a host reporting `position == 0` fails the whole match
        // rather than silently shifting every other agent's update.
        let mut ranks: Vec<(AgentId, Rank)> = Vec::with_capacity(result.placements.len());
        for p in &result.placements {
            match Rank::new(p.position) {
                Some(rank) => ranks.push((p.agent_id, rank)),
                None => {
                    tracing::warn!(
                        match_id,
                        agent_id = p.agent_id,
                        position = p.position,
                        "Skipping rating update: rank must be >= 1"
                    );
                    return;
                }
            }
        }
        let ids: Vec<AgentId> = result.placements.iter().map(|p| p.agent_id).collect();
        let stored = match self.load_ratings_retry(&ids).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(match_id, error = %e, "Failed to load ratings; skipping update");
                return;
            }
        };
        let fallback = StoredRating::default_rating();
        let players: Vec<FfaPlayer> = result
            .placements
            .iter()
            .zip(ranks.iter())
            .map(|(p, (_, rank))| {
                let s = stored.get(&p.agent_id).copied().unwrap_or(fallback);
                FfaPlayer {
                    agent_id: p.agent_id,
                    rating: WengLinRating {
                        rating: s.rating,
                        uncertainty: s.uncertainty,
                    },
                    rank: *rank,
                }
            })
            .collect();
        // Defense in depth: already checked above, but `rate_ffa` re-checks
        // length and uniqueness so a future caller can't bypass them.
        if let Err(e) = ValidatedFfaPlayers::try_from(players.as_slice()) {
            tracing::warn!(match_id, error = %e, "Skipping rating update");
            return;
        }
        let rated = match ranking::rate_ffa(&players, &self.config.weng_lin_config) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(match_id, error = %e, "Skipping rating update");
                return;
            }
        };
        // Join on `agent_id`, not positionally: correct even if the rating
        // backend ever reordered its output.
        let rated_by_id: HashMap<AgentId, ranking::RatedPlayer> =
            rated.into_iter().map(|r| (r.agent_id, r)).collect();
        let mut placements: Vec<FinishedPlacement> = Vec::with_capacity(result.placements.len());
        for p in &result.placements {
            match rated_by_id.get(&p.agent_id) {
                Some(r) => placements.push(FinishedPlacement {
                    agent_id: p.agent_id,
                    position: p.position,
                    score: p.score,
                    old_rating: StoredRating {
                        rating: r.old_rating.rating,
                        uncertainty: r.old_rating.uncertainty,
                    },
                    new_rating: StoredRating {
                        rating: r.new_rating.rating,
                        uncertainty: r.new_rating.uncertainty,
                    },
                }),
                None => {
                    tracing::error!(
                        match_id,
                        agent_id = p.agent_id,
                        "Skipping rating update: rated output missing agent"
                    );
                    return;
                }
            }
        }
        if let Err(e) = self.record_match_retry(match_id, &placements).await {
            tracing::error!(match_id, error = %e, "Failed to record match ratings");
        }
    }

    /// Retry delays for best-effort rating I/O. Short enough to not stall
    /// the match loop, long enough to ride out a DB blip.
    fn rating_retry_delays() -> [Duration; 3] {
        [
            Duration::from_millis(100),
            Duration::from_millis(400),
            Duration::from_millis(1000),
        ]
    }

    async fn load_ratings_retry(
        &self,
        ids: &[AgentId],
    ) -> Result<HashMap<AgentId, StoredRating>, Box<dyn std::error::Error + Send + Sync>> {
        let delays = Self::rating_retry_delays();
        let mut attempt = 0usize;
        loop {
            match self.match_recorder.load_ratings(ids).await {
                Ok(s) => return Ok(s),
                Err(e) if attempt < delays.len() => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        "load_ratings failed; retrying"
                    );
                    tokio::time::sleep(delays[attempt]).await;
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn record_match_retry(
        &self,
        match_id: &str,
        placements: &[FinishedPlacement],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let delays = Self::rating_retry_delays();
        let mut attempt = 0usize;
        loop {
            match self
                .match_recorder
                .record_finished_match(match_id, placements)
                .await
            {
                Ok(()) => return Ok(()),
                Err(e) if attempt < delays.len() => {
                    tracing::warn!(
                        match_id,
                        attempt = attempt + 1,
                        error = %e,
                        "record_finished_match failed; retrying"
                    );
                    tokio::time::sleep(delays[attempt]).await;
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Spawn all machines, run the game, then destroy all machines.
    ///
    /// Returns before `cleanup_match` — the caller handles that so it always runs.
    async fn run_game_inner(
        &self,
        ctx: &P::MatchContext,
        layout: MatchLayout,
        agents: &[AgentInfo],
    ) -> Result<GameResult, CoordinatorError> {
        // Spawn game host
        let game_host_handle = self.spawn_game_host(ctx).await?;
        tracing::info!("Game host spawned at {}", game_host_handle.private_ip);

        // Spawn agents, cleaning up on failure. Slots come from the validated
        // layout, so they are always in range — no manual `i + 1` arithmetic.
        debug_assert_eq!(agents.len(), layout.all_agent_slots().len());
        let mut agent_handles: Vec<(AgentId, MachineHandle)> = Vec::new();
        for (agent, slot) in agents.iter().zip(layout.all_agent_slots()) {
            match self.spawn_agent(ctx, agent, slot).await {
                Ok(handle) => {
                    tracing::info!(
                        agent_id = agent.id,
                        ip = handle.private_ip,
                        slot = slot.raw_slot(),
                        "Agent spawned"
                    );
                    agent_handles.push((agent.id, handle));
                }
                Err(e) => {
                    tracing::error!(agent_id = agent.id, "Failed to spawn agent: {}", e);
                    self.destroy_all(ctx, Some(&game_host_handle), &agent_handles)
                        .await;
                    return Err(e);
                }
            }
        }

        // Run the game (lineup order == StartGame agent order == player slots).
        let game_result = self
            .run_game(&game_host_handle, &agent_handles, agents)
            .await;

        // Destroy machines regardless of game outcome
        self.destroy_all(ctx, Some(&game_host_handle), &agent_handles)
            .await;

        game_result
    }

    async fn spawn_game_host(
        &self,
        ctx: &P::MatchContext,
    ) -> Result<MachineHandle, CoordinatorError> {
        // Game host is on a public registry, no copy or token needed. The game
        // parameters (player count, tick rate, arena size) travel in the typed
        // `StartGame` config, not env vars, so there is nothing to set here.
        let config = HostSpawnConfig::new(
            ContainerImage::Public(self.config.game_host_image.clone()),
            self.config.game_host_grpc_port,
        );

        self.machine_provider
            .spawn_host(ctx, config)
            .await
            .map_err(CoordinatorError::MachineSpawn)
    }

    async fn spawn_agent(
        &self,
        ctx: &P::MatchContext,
        agent: &AgentInfo,
        slot: AgentSlot,
    ) -> Result<MachineHandle, CoordinatorError> {
        // Agents are pulled from the private registry with a scoped deploy token.
        let registry_token = self
            .token_provider
            .get_deploy_token(&agent.image_url)
            .await
            .map_err(CoordinatorError::DeployToken)?;
        let container_image = ContainerImage::Private {
            image_url: agent.image_url.to_image_url(),
            registry_token,
        };

        let config = AgentSpawnConfig::new(container_image, slot, self.config.agent_grpc_port);

        self.machine_provider
            .spawn_agent(ctx, config)
            .await
            .map_err(CoordinatorError::MachineSpawn)
    }

    async fn run_game(
        &self,
        game_host: &MachineHandle,
        agents: &[(AgentId, MachineHandle)],
        lineup_info: &[AgentInfo],
    ) -> Result<GameResult, CoordinatorError> {
        // A backend that relays through a published host port reports the port
        // to dial; otherwise the machine is addressed directly on the port its
        // workload listens on.
        let game_host_addr = format!(
            "http://{}:{}",
            game_host.private_ip,
            game_host
                .grpc_port
                .unwrap_or(self.config.game_host_grpc_port)
        );

        let mut client = self.connect_game_host(&game_host_addr).await?;

        let agent_endpoints: Vec<AgentEndpoint> = agents
            .iter()
            .map(|(id, handle)| AgentEndpoint {
                agent_id: *id,
                // Consumed by the game host, which dials agents itself and
                // retries while they boot.
                address: format!(
                    "{}:{}",
                    handle.private_ip,
                    handle.grpc_port.unwrap_or(self.config.agent_grpc_port)
                ),
            })
            .collect();

        let start_request = StartGameRequest {
            agents: agent_endpoints,
            config: Some(GameConfig {
                tick_rate_ms: self.config.tick_rate_ms,
                arena_width: self.config.arena_width,
                arena_height: self.config.arena_height,
            }),
        };

        client
            .start_game(start_request)
            .await
            .map_err(|e| CoordinatorError::GameHost(e.to_string()))?;

        tracing::info!("Game started");

        // Publish the game host + lineup so the website's spectator relay can
        // stream frames and render a color legend. The guard clears the address
        // when it drops — on normal completion, early return, cancellation, or
        // panic — so a stale game host never lingers between games, while the
        // lineup + terminal result are retained for the overlay. Reuse the exact
        // address form used to dial above.
        let lineup: Vec<LineupEntry> = lineup_info
            .iter()
            .enumerate()
            .map(|(slot, a)| LineupEntry {
                slot,
                agent_id: a.id,
                name: a.name.clone(),
            })
            .collect();
        let registry_guard = SpectatorRegistryGuard::publish(
            self.spectator_registry.clone(),
            GameHostAddr::new(game_host_addr),
            lineup,
        )
        .await;

        // Poll until the game ends. The result is published to the registry
        // *before* returning so the relay can emit a `result` SSE event even
        // though the game host is destroyed right after this returns.
        match self.poll_until_done(&mut client).await {
            Ok(result) => {
                registry_guard
                    .set_result(SpectatorResult {
                        placements: result.placements.clone(),
                        error: String::new(),
                    })
                    .await;
                Ok(result)
            }
            Err(e) => {
                registry_guard
                    .set_result(SpectatorResult {
                        placements: Vec::new(),
                        error: e.to_string(),
                    })
                    .await;
                Err(e)
            }
        }
    }

    /// Dial the game host, retrying until it answers or
    /// [`CoordinatorConfig::game_host_connect_timeout`] elapses.
    ///
    /// The wait is unavoidable — the machine has to boot and the workload has to
    /// bind its listener — but its length is backend-dependent (a container
    /// start versus a microVM boot plus an image pull), so retrying until
    /// success both starts as soon as possible and tolerates a slow boot. This
    /// mirrors what the game host already does when dialing agents.
    async fn connect_game_host(
        &self,
        addr: &str,
    ) -> Result<GameHostClient<tonic::transport::Channel>, CoordinatorError> {
        const RETRY_INTERVAL: Duration = Duration::from_millis(500);

        let deadline = tokio::time::Instant::now() + self.config.game_host_connect_timeout;
        let mut attempts = 0u32;
        loop {
            attempts += 1;
            match GameHostClient::connect(addr.to_string()).await {
                Ok(client) => {
                    tracing::info!(addr, attempts, "Connected to game host");
                    return Ok(client);
                }
                Err(e) => {
                    if tokio::time::Instant::now() + RETRY_INTERVAL >= deadline {
                        // Report the spent budget: the usual cause is a workload
                        // that never bound its port, not a slow boot.
                        return Err(CoordinatorError::Connection(format!(
                            "game host at {addr} unreachable after {attempts} attempts over {:?}: {e}",
                            self.config.game_host_connect_timeout
                        )));
                    }
                    tracing::debug!(addr, attempts, error = %e, "Game host not up yet; retrying");
                    tokio::time::sleep(RETRY_INTERVAL).await;
                }
            }
        }
    }

    /// Poll `GetStatus` on `poll_interval` until the game finishes or fails.
    async fn poll_until_done(
        &self,
        client: &mut GameHostClient<tonic::transport::Channel>,
    ) -> Result<GameResult, CoordinatorError> {
        loop {
            tokio::time::sleep(self.config.poll_interval).await;

            let status = client
                .get_status(GetStatusRequest {})
                .await
                .map_err(|e| CoordinatorError::GameHost(e.to_string()))?
                .into_inner();

            match status.state() {
                GameState::Running => {
                    tracing::debug!("Game running, tick {}", status.current_tick);
                }
                GameState::WaitingForAgents => {
                    tracing::debug!("Waiting for agents to connect...");
                }
                GameState::Finished => {
                    let result = status.result.ok_or_else(|| {
                        CoordinatorError::GameHost("Game finished but no result".into())
                    })?;
                    return Ok(GameResult {
                        winner_agent_id: result.placements.first().map(|p| p.agent_id),
                        placements: result
                            .placements
                            .into_iter()
                            .map(|p| AgentPlacement {
                                agent_id: p.agent_id,
                                position: p.position,
                                score: p.score,
                            })
                            .collect(),
                    });
                }
                GameState::Failed => {
                    let error = status
                        .result
                        .map(|r| r.error)
                        .unwrap_or_else(|| "Unknown error".into());
                    return Err(CoordinatorError::GameHost(error));
                }
                GameState::Unspecified => {
                    return Err(CoordinatorError::GameHost("Unknown game state".into()));
                }
            }
        }
    }

    /// Destroy all spawned machines. Best-effort: logs errors but does not abort.
    async fn destroy_all(
        &self,
        ctx: &P::MatchContext,
        game_host: Option<&MachineHandle>,
        agents: &[(AgentId, MachineHandle)],
    ) {
        use futures_util::future::join_all;

        let mut targets: Vec<(String, &MachineHandle)> =
            Vec::with_capacity(agents.len() + usize::from(game_host.is_some()));
        if let Some(handle) = game_host {
            targets.push(("game host".to_string(), handle));
        }
        for (agent_id, handle) in agents {
            targets.push((format!("agent {agent_id}"), handle));
        }

        join_all(targets.into_iter().map(|(label, handle)| async move {
            if let Err(e) = self.machine_provider.destroy(ctx, handle).await {
                tracing::error!("Failed to destroy {label}: {e}");
            }
        }))
        .await;
    }
}

/// Result of a completed game
#[derive(Debug, Clone)]
pub struct GameResult {
    pub winner_agent_id: Option<AgentId>,
    pub placements: Vec<AgentPlacement>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentPlacement {
    pub agent_id: AgentId,
    pub position: u32,
    pub score: u32,
}

/// Errors that can occur during coordination
#[derive(Debug, thiserror::Error)]
pub enum CoordinatorError {
    #[error("Database error: {0}")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Failed to spawn machine: {0}")]
    MachineSpawn(#[from] MachineError),

    #[error("Failed to get deploy token: {0}")]
    DeployToken(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Game host error: {0}")]
    GameHost(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct StubProvider;

    #[async_trait::async_trait]
    impl agent_infra::MachineProvider for StubProvider {
        type MatchContext = ();

        async fn init_match(
            &self,
            _match_id: &str,
            _layout: MatchLayout,
        ) -> Result<Self::MatchContext, MachineError> {
            Ok(())
        }

        async fn spawn_host(
            &self,
            _ctx: &Self::MatchContext,
            _config: agent_infra::HostSpawnConfig,
        ) -> Result<MachineHandle, MachineError> {
            Err(MachineError::MachineCreation("stub".into()))
        }

        async fn spawn_agent(
            &self,
            _ctx: &Self::MatchContext,
            _config: agent_infra::AgentSpawnConfig,
        ) -> Result<MachineHandle, MachineError> {
            Err(MachineError::MachineCreation("stub".into()))
        }

        async fn destroy(
            &self,
            _ctx: &Self::MatchContext,
            _handle: &MachineHandle,
        ) -> Result<(), MachineError> {
            Ok(())
        }

        async fn cleanup_match(&self, _ctx: Self::MatchContext) -> Result<(), MachineError> {
            Ok(())
        }

        async fn list_orphaned(
            &self,
            _prefix: &str,
            _max_age: Duration,
        ) -> Result<Vec<agent_infra::OrphanedResource>, MachineError> {
            Ok(vec![])
        }

        async fn destroy_orphaned(
            &self,
            _resource: &agent_infra::OrphanedResource,
        ) -> Result<(), MachineError> {
            Ok(())
        }
    }

    struct StubRepo;

    #[async_trait::async_trait]
    impl AgentRepository for StubRepo {
        async fn get_random_active_agents(
            &self,
            _count: usize,
        ) -> Result<Vec<AgentInfo>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(vec![])
        }
    }

    struct StubTokens;

    #[async_trait::async_trait]
    impl DeployTokenProvider for StubTokens {
        async fn get_deploy_token(
            &self,
            _image: &(dyn ContainerImageUrl + Send + Sync),
        ) -> Result<common::RegistryToken, Box<dyn std::error::Error + Send + Sync>> {
            Err("stub".into())
        }
    }

    struct StubRecorder {
        ratings: HashMap<AgentId, StoredRating>,
        recorded: Mutex<Vec<(String, Vec<FinishedPlacement>)>>,
    }

    impl StubRecorder {
        fn with_defaults() -> Self {
            Self {
                ratings: HashMap::new(),
                recorded: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait::async_trait]
    impl MatchRecorder for StubRecorder {
        async fn load_ratings(
            &self,
            agent_ids: &[AgentId],
        ) -> Result<HashMap<AgentId, StoredRating>, Box<dyn std::error::Error + Send + Sync>>
        {
            let fallback = StoredRating::default_rating();
            Ok(agent_ids
                .iter()
                .map(|id| (*id, self.ratings.get(id).copied().unwrap_or(fallback)))
                .collect())
        }

        async fn record_finished_match(
            &self,
            external_match_id: &str,
            placements: &[FinishedPlacement],
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.recorded
                .lock()
                .unwrap()
                .push((external_match_id.to_string(), placements.to_vec()));
            Ok(())
        }
    }

    fn test_config() -> CoordinatorConfig {
        CoordinatorConfig {
            game_host_image: ImageUrl::new("test/host:latest".to_string()).unwrap(),
            agents_per_game: 4,
            tick_rate_ms: 50,
            arena_width: 1000,
            arena_height: 1000,
            game_interval: Duration::from_secs(1),
            poll_interval: Duration::from_millis(10),
            game_host_grpc_port: 50051,
            agent_grpc_port: 50052,
            game_host_connect_timeout: Duration::from_secs(1),
            weng_lin_config: default_weng_lin_config(),
        }
    }

    fn test_coordinator(
        recorder: StubRecorder,
    ) -> (GameCoordinator<StubProvider>, Arc<StubRecorder>) {
        let recorder = Arc::new(recorder);
        // The coordinator boxes its own handle; share the stub through an
        // `Arc` wrapper so the test can inspect recorded calls afterwards.
        struct Shared(Arc<StubRecorder>);
        #[async_trait::async_trait]
        impl MatchRecorder for Shared {
            async fn load_ratings(
                &self,
                ids: &[AgentId],
            ) -> Result<HashMap<AgentId, StoredRating>, Box<dyn std::error::Error + Send + Sync>>
            {
                self.0.load_ratings(ids).await
            }
            async fn record_finished_match(
                &self,
                id: &str,
                placements: &[FinishedPlacement],
            ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                self.0.record_finished_match(id, placements).await
            }
        }
        let coordinator = GameCoordinator::new(
            test_config(),
            Arc::new(StubProvider),
            Box::new(StubRepo),
            Box::new(StubTokens),
            Box::new(Shared(recorder.clone())),
            Arc::new(RwLock::new(SpectatorMatch::default())),
        );
        (coordinator, recorder)
    }

    fn finished_result() -> GameResult {
        GameResult {
            winner_agent_id: Some(1),
            placements: vec![
                AgentPlacement {
                    agent_id: 1,
                    position: 1,
                    score: 100,
                },
                AgentPlacement {
                    agent_id: 2,
                    position: 2,
                    score: 70,
                },
                AgentPlacement {
                    agent_id: 3,
                    position: 3,
                    score: 40,
                },
                AgentPlacement {
                    agent_id: 4,
                    position: 4,
                    score: 10,
                },
            ],
        }
    }

    /// The guard clears the address on drop (stale hosts never linger) but
    /// retains the lineup + terminal result for the spectator overlay.
    #[tokio::test]
    async fn registry_guard_retains_lineup_and_result_on_drop() {
        let registry: SpectatorRegistry = Arc::new(RwLock::new(SpectatorMatch::default()));
        let lineup = vec![LineupEntry {
            slot: 0,
            agent_id: 1,
            name: "alpha".into(),
        }];
        let guard = SpectatorRegistryGuard::publish(
            registry.clone(),
            GameHostAddr::new("http://game-host:50051"),
            lineup.clone(),
        )
        .await;
        assert_eq!(
            registry.read().await.addr,
            Some(GameHostAddr::new("http://game-host:50051"))
        );
        assert_eq!(registry.read().await.lineup, lineup);
        assert!(registry.read().await.last_result.is_none());

        guard
            .set_result(SpectatorResult {
                placements: vec![AgentPlacement {
                    agent_id: 1,
                    position: 1,
                    score: 10,
                }],
                error: String::new(),
            })
            .await;

        drop(guard);
        // Drop hands the clear to the runtime; poll briefly for it.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            {
                let m = registry.read().await;
                if m.addr.is_none() {
                    assert_eq!(m.lineup, lineup);
                    let result = m.last_result.as_ref().expect("result retained");
                    assert_eq!(result.placements.len(), 1);
                    assert_eq!(result.placements[0].agent_id, 1);
                    return;
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "registry address was not cleared after guard drop"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// A finished game records one rating update per placement: the winner
    /// gains, the loser loses, and the external match id is passed through.
    #[tokio::test]
    async fn update_ratings_records_ffa_result() {
        let (coordinator, recorder) = test_coordinator(StubRecorder::with_defaults());
        coordinator
            .update_ratings("match-1", &finished_result())
            .await;

        let recorded = recorder.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "match-1");
        let placements = &recorded[0].1;
        assert_eq!(placements.len(), 4);
        for (placed, recorded) in finished_result().placements.iter().zip(placements.iter()) {
            assert_eq!(placed.agent_id, recorded.agent_id);
            assert_eq!(placed.position, recorded.position);
            assert_eq!(placed.score, recorded.score);
        }
        let winner = placements.iter().find(|p| p.agent_id == 1).unwrap();
        let loser = placements.iter().find(|p| p.agent_id == 4).unwrap();
        assert!(winner.new_rating.rating > winner.old_rating.rating);
        assert!(loser.new_rating.rating < loser.old_rating.rating);
    }

    /// Agents with no stored rating are scored from the default (1500) and
    /// still recorded — first matches need no special casing.
    #[tokio::test]
    async fn update_ratings_defaults_missing_ratings() {
        let (coordinator, recorder) = test_coordinator(StubRecorder::with_defaults());
        coordinator
            .update_ratings("match-2", &finished_result())
            .await;

        let recorded = recorder.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        for p in &recorded[0].1 {
            assert!((p.old_rating.rating - 1500.0).abs() < f64::EPSILON);
            assert!((p.old_rating.uncertainty - 500.0).abs() < f64::EPSILON);
        }
    }

    /// An empty result records nothing instead of erroring (e.g. a game that
    /// finished with no usable placements).
    #[tokio::test]
    async fn update_ratings_skips_empty_result() {
        let (coordinator, recorder) = test_coordinator(StubRecorder::with_defaults());
        coordinator
            .update_ratings(
                "match-3",
                &GameResult {
                    winner_agent_id: None,
                    placements: vec![],
                },
            )
            .await;

        assert!(recorder.recorded.lock().unwrap().is_empty());
    }

    /// The mirrored default-rating literals stay in sync: canonical values
    /// live in `common`, `achtung-ranking` keeps a leaf-local copy.
    #[test]
    fn default_ratings_match_common() {
        let math = ranking::default_rating();
        let stored = StoredRating::default_rating();
        assert!((math.rating - stored.rating).abs() < f64::EPSILON);
        assert!((math.uncertainty - stored.uncertainty).abs() < 1e-12);
        assert!((ranking::DEFAULT_RATING - common::DEFAULT_RATING).abs() < f64::EPSILON);
        assert!(
            (ranking::DEFAULT_UNCERTAINTY - common::DEFAULT_UNCERTAINTY).abs() < 1e-12,
            "ranking/common default uncertainty diverged"
        );
        assert_eq!(
            ranking::format_rating(&math),
            StoredRating {
                rating: math.rating,
                uncertainty: math.uncertainty,
            }
            .format()
        );
        assert_eq!(ranking::format_rating(&math), "1500");
    }

    /// The shipped config matches the Elo-scale defaults (never the raw
    /// upstream beta, which would saturate win probabilities at this scale).
    #[test]
    fn default_config_is_elo_scaled() {
        // `WengLinConfig` has no `PartialEq`; compare field by field.
        let (shipped, canonical) = (default_weng_lin_config(), ranking::default_config());
        assert_eq!(shipped.beta, 250.0);
        assert_eq!(shipped.beta, canonical.beta);
        assert_eq!(
            shipped.uncertainty_tolerance,
            canonical.uncertainty_tolerance
        );
    }

    /// A zero rank from the host fails the match before any I/O — no
    /// ratings and no history row for that match.
    #[tokio::test]
    async fn update_ratings_skips_invalid_rank() {
        let (coordinator, recorder) = test_coordinator(StubRecorder::with_defaults());
        let mut bad = finished_result();
        bad.placements[1].position = 0;
        coordinator.update_ratings("match-bad-rank", &bad).await;

        assert!(recorder.recorded.lock().unwrap().is_empty());
    }

    /// A duplicated agent fails the match before any I/O instead of dying
    /// opaquely on the DB primary key.
    #[tokio::test]
    async fn update_ratings_skips_duplicate_agent() {
        let (coordinator, recorder) = test_coordinator(StubRecorder::with_defaults());
        let mut dup = finished_result();
        dup.placements[1].agent_id = dup.placements[0].agent_id;
        coordinator.update_ratings("match-dup", &dup).await;

        assert!(recorder.recorded.lock().unwrap().is_empty());
    }

    /// A single placement cannot be rated and records nothing.
    #[tokio::test]
    async fn update_ratings_skips_single_player() {
        let (coordinator, recorder) = test_coordinator(StubRecorder::with_defaults());
        coordinator
            .update_ratings(
                "match-solo",
                &GameResult {
                    winner_agent_id: Some(1),
                    placements: vec![AgentPlacement {
                        agent_id: 1,
                        position: 1,
                        score: 100,
                    }],
                },
            )
            .await;

        assert!(recorder.recorded.lock().unwrap().is_empty());
    }

    /// Transient load failures are retried: a recorder that fails twice
    /// then succeeds still records the match.
    #[tokio::test]
    async fn update_ratings_retries_transient_load_failure() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Flaky {
            calls: AtomicUsize,
            inner: StubRecorder,
        }
        #[async_trait::async_trait]
        impl MatchRecorder for Flaky {
            async fn load_ratings(
                &self,
                ids: &[AgentId],
            ) -> Result<HashMap<AgentId, StoredRating>, Box<dyn std::error::Error + Send + Sync>>
            {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    return Err("blip".into());
                }
                self.inner.load_ratings(ids).await
            }
            async fn record_finished_match(
                &self,
                id: &str,
                placements: &[FinishedPlacement],
            ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                self.inner.record_finished_match(id, placements).await
            }
        }

        let flaky = Arc::new(Flaky {
            calls: AtomicUsize::new(0),
            inner: StubRecorder::with_defaults(),
        });
        struct Shared(Arc<Flaky>);
        #[async_trait::async_trait]
        impl MatchRecorder for Shared {
            async fn load_ratings(
                &self,
                ids: &[AgentId],
            ) -> Result<HashMap<AgentId, StoredRating>, Box<dyn std::error::Error + Send + Sync>>
            {
                self.0.load_ratings(ids).await
            }
            async fn record_finished_match(
                &self,
                id: &str,
                placements: &[FinishedPlacement],
            ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                self.0.record_finished_match(id, placements).await
            }
        }
        let coordinator = GameCoordinator::new(
            test_config(),
            Arc::new(StubProvider),
            Box::new(StubRepo),
            Box::new(StubTokens),
            Box::new(Shared(flaky.clone())),
            Arc::new(RwLock::new(SpectatorMatch::default())),
        );
        coordinator
            .update_ratings("match-retry", &finished_result())
            .await;

        assert_eq!(flaky.calls.load(Ordering::SeqCst), 3);
        assert_eq!(flaky.inner.recorded.lock().unwrap().len(), 1);
    }
}
