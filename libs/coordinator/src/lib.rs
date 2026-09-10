use std::sync::Arc;
use std::time::Duration;

use agent_infra::{
    AgentSlot, AgentSpawnConfig, ContainerImage, HostSpawnConfig, MachineError, MachineHandle,
    MachineProvider, MatchLayout,
};
use common::{AgentId, AgentInfo, AgentRepository, ContainerImageUrl, DeployTokenProvider};
use game_host::game_host_client::GameHostClient;
use game_host::{AgentEndpoint, GameConfig, GameState, GetStatusRequest, StartGameRequest};
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

/// The game host currently hosting a match, or `None` between games. Written by
/// the coordinator, read by the spectator relay. One game runs at a time, so a
/// single slot suffices.
pub type SpectatorRegistry = Arc<RwLock<Option<GameHostAddr>>>;

/// Publishes a game host to the [`SpectatorRegistry`] for its lifetime and
/// clears it on drop, so a cancelled or panicking match never leaves a stale
/// address pointing at a destroyed host. Clearing an explicit `None` at the end
/// of the happy path is not enough: match tasks can be cancelled (e.g. a match
/// timeout) between publish and clear.
struct SpectatorRegistryGuard {
    registry: SpectatorRegistry,
    addr: GameHostAddr,
}

impl SpectatorRegistryGuard {
    /// Publish `addr` and return a guard that clears it on drop.
    async fn publish(registry: SpectatorRegistry, addr: GameHostAddr) -> Self {
        *registry.write().await = Some(addr.clone());
        Self { registry, addr }
    }
}

impl Drop for SpectatorRegistryGuard {
    fn drop(&mut self) {
        // Drop can't await, so hand the clear to the runtime. Only clear if we
        // are still the published address, so a newer game isn't clobbered.
        let registry = self.registry.clone();
        let addr = std::mem::take(&mut self.addr);
        tokio::spawn(async move {
            let mut slot = registry.write().await;
            if slot.as_ref() == Some(&addr) {
                *slot = None;
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
    /// Publishes the current game host address for the spectator relay.
    spectator_registry: SpectatorRegistry,
}

impl<P: MachineProvider> GameCoordinator<P> {
    pub fn new(
        config: CoordinatorConfig,
        machine_provider: Arc<P>,
        agent_repo: Box<dyn AgentRepository>,
        token_provider: Box<dyn DeployTokenProvider>,
        spectator_registry: SpectatorRegistry,
    ) -> Self {
        Self {
            config,
            machine_provider,
            agent_repo,
            token_provider,
            spectator_registry,
        }
    }

    /// Run a single game to completion (used for testing / one-shot runs).
    ///
    /// Teardown runs in the background for the loop, but `run_once` awaits it
    /// so callers see a fully cleaned-up match on return.
    pub async fn run_once(&self) -> Result<(), CoordinatorError> {
        let (result, teardown) = self.run_single_game().await;
        if let Some(handle) = teardown {
            // Teardown errors are already logged inside the task; only surface
            // a panic / cancellation here.
            match handle.await {
                Ok(()) => {}
                Err(e) => {
                    return Err(CoordinatorError::GameHost(format!(
                        "teardown task failed: {e}"
                    )));
                }
            }
        }
        result
    }

    /// Spawn the coordinator as a background task
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Main coordinator loop
    ///
    /// Teardown of match N runs in the background while the loop sleeps, and
    /// the next iteration awaits the previous teardown before spawning. In the
    /// common case (parallel teardown of ~2s < `game_interval`) that await is
    /// instant, so the visible gap is just `game_interval`. Awaiting guarantees
    /// the fixed relay ports (`base + slot`, no allocator) are free before the
    /// next `spawn_*`; leftovers from a failed/cancelled teardown are still
    /// collected by the pre-flight sweep and the reaper.
    async fn run(self) {
        tracing::info!("Game coordinator started");

        let mut prev_teardown: Option<JoinHandle<()>> = None;
        loop {
            // Previous teardown ran during the last sleep, so this is usually
            // instant. Awaiting here (rather than firing and forgetting)
            // guarantees ports are free before the next spawn.
            if let Some(handle) = prev_teardown.take()
                && let Err(e) = handle.await
            {
                tracing::warn!(error = %e, "Previous match teardown task failed");
            }

            let (outcome, teardown) = self.run_single_game().await;
            match outcome {
                Ok(()) => {
                    tracing::info!("Game completed successfully");
                }
                Err(e) => {
                    tracing::error!("Game failed: {}", e);
                }
            }
            prev_teardown = teardown;

            tokio::time::sleep(self.config.game_interval).await;
        }
    }

    /// Run a single game from start to finish.
    ///
    /// Returns the game outcome plus a handle to the background teardown task
    /// (destroy + cleanup). The caller decides when to await it: the loop
    /// overlaps it with the inter-game sleep, `run_once` awaits it inline.
    /// `None` means there is nothing to tear down (no match was initialized).
    async fn run_single_game(&self) -> (Result<(), CoordinatorError>, Option<JoinHandle<()>>) {
        // 1. Pick agents from the roster
        let agents = match self
            .agent_repo
            .get_random_active_agents(self.config.agents_per_game)
            .await
        {
            Ok(agents) => agents,
            Err(e) => return (Err(CoordinatorError::Database(e)), None),
        };

        if agents.len() < self.config.agents_per_game {
            tracing::warn!(
                "Not enough active agents ({}/{}), skipping game",
                agents.len(),
                self.config.agents_per_game
            );
            return (Ok(()), None);
        }

        tracing::info!("Starting game with {} agents", agents.len());

        // 2. Initialize match infrastructure (network, etc.)
        let match_id = agent_infra::generate_id();
        // Validated once: rejects zero agents and counts that would overflow
        // the u8 wire slot or the relay port range.
        let layout = match MatchLayout::new(agents.len()) {
            Ok(layout) => layout,
            Err(e) => return (Err(CoordinatorError::MachineSpawn(e)), None),
        };
        let ctx = match self.machine_provider.init_match(&match_id, layout).await {
            Ok(ctx) => ctx,
            Err(e) => return (Err(CoordinatorError::MachineSpawn(e)), None),
        };

        // 3. Spawn machines and run the game. This returns the handles without
        // destroying them, so teardown below can run in the background.
        let outcome = self.run_match_foreground(&ctx, layout, &agents).await;

        // 4. Teardown (destroy + cleanup) always runs in the background, even
        // on spawn/poll failure — `outcome` carries whatever was created.
        // The "Game finished" log below fires as soon as the result is known
        // (~12s earlier than before); completion of the teardown itself is
        // logged inside the task.
        let teardown = self.spawn_teardown(ctx, outcome.game_host, outcome.agents, match_id);

        match outcome.result {
            Ok(result) => {
                tracing::info!(
                    "Game finished: {:?} (teardown running in background)",
                    result
                );
                // TODO: Record results in database
                (Ok(()), Some(teardown))
            }
            Err(e) => (Err(e), Some(teardown)),
        }
    }

    /// Spawn all machines and run the game, returning the outcome plus
    /// whatever machines were created (possibly partial on spawn failure).
    ///
    /// Destroying those machines is the caller's job (background teardown), so
    /// this returns before any `destroy`/`cleanup_match` runs.
    async fn run_match_foreground(
        &self,
        ctx: &P::MatchContext,
        layout: MatchLayout,
        agents: &[AgentInfo],
    ) -> MatchOutcome {
        // Spawn game host
        let game_host_handle = match self.spawn_game_host(ctx).await {
            Ok(handle) => {
                tracing::info!("Game host spawned at {}", handle.private_ip);
                handle
            }
            Err(e) => {
                return MatchOutcome {
                    result: Err(e),
                    game_host: None,
                    agents: Vec::new(),
                };
            }
        };

        // Spawn agents. Slots come from the validated layout, so they are
        // always in range — no manual `i + 1` arithmetic. On failure the
        // partial set is returned for the background teardown; nothing is
        // destroyed here.
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
                    return MatchOutcome {
                        result: Err(e),
                        game_host: Some(game_host_handle),
                        agents: agent_handles,
                    };
                }
            }
        }

        // Run the game
        let game_result = self.run_game(&game_host_handle, &agent_handles).await;

        MatchOutcome {
            result: game_result,
            game_host: Some(game_host_handle),
            agents: agent_handles,
        }
    }

    /// Hand owned machines + match context to a background task that destroys
    /// them concurrently and then runs `cleanup_match`. Best-effort like the
    /// old foreground path: per-machine errors are logged, not propagated.
    ///
    /// Requires `P::MatchContext: 'static` (see [`MachineProvider`]) so the
    /// task can own it.
    fn spawn_teardown(
        &self,
        ctx: P::MatchContext,
        game_host: Option<MachineHandle>,
        agents: Vec<(AgentId, MachineHandle)>,
        match_id: String,
    ) -> JoinHandle<()> {
        let provider = Arc::clone(&self.machine_provider);
        tokio::spawn(async move {
            let start = std::time::Instant::now();
            let machine_count = agents.len() + game_host.as_ref().map_or(0, |_| 1);

            Self::destroy_all(&provider, &ctx, game_host.as_ref(), &agents).await;

            if let Err(e) = provider.cleanup_match(ctx).await {
                tracing::error!("Failed to cleanup match {}: {}", match_id, e);
            }

            tracing::debug!(
                match_id,
                machines = machine_count,
                elapsed_ms = start.elapsed().as_millis(),
                "Background teardown complete"
            );
        })
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

        // Publish the game host so the website's spectator relay can stream it.
        // The guard clears the registry when it drops — on normal completion,
        // early return, cancellation, or panic — so a stale game host never
        // lingers between games. Reuse the exact address form used to dial above.
        let _registry_guard = SpectatorRegistryGuard::publish(
            self.spectator_registry.clone(),
            GameHostAddr::new(game_host_addr),
        )
        .await;

        // Poll until the game ends.
        self.poll_until_done(&mut client).await
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

    /// Destroy all spawned machines concurrently. Best-effort: logs errors
    /// but does not abort.
    ///
    /// Sequential `stop + remove` costs ~2s per microVM (~12s for host + 5
    /// agents); the per-machine destroys are independent (distinct sandbox
    /// names, idempotent via "already gone" tolerance), so `join_all` pays
    /// roughly the slowest single destroy instead of the sum.
    async fn destroy_all(
        provider: &P,
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
            if let Err(e) = provider.destroy(ctx, handle).await {
                tracing::error!("Failed to destroy {label}: {e}");
            }
        }))
        .await;
    }
}

/// Machines created for one match plus its outcome.
///
/// Produced by the foreground phase (spawn + poll) and consumed by the
/// background teardown task. Handles may be partial when spawning failed
/// partway through.
struct MatchOutcome {
    result: Result<GameResult, CoordinatorError>,
    game_host: Option<MachineHandle>,
    agents: Vec<(AgentId, MachineHandle)>,
}

/// Result of a completed game
#[derive(Debug)]
pub struct GameResult {
    pub winner_agent_id: Option<AgentId>,
    pub placements: Vec<AgentPlacement>,
}

#[derive(Debug)]
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
