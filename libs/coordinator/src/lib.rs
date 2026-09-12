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
                // TODO: Record results in database
                Ok(())
            }
            Err(e) => Err(e),
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
}
