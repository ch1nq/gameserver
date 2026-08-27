use std::sync::Arc;
use std::time::Duration;

use agent_infra::{ContainerImage, MachineError, MachineHandle, MachineProvider, SpawnConfig};
use common::{AgentId, AgentInfo, AgentRepository, ContainerImageUrl, DeployTokenProvider};
use game_host::game_host_client::GameHostClient;
use game_host::{AgentEndpoint, GameConfig, GameState, GetStatusRequest, StartGameRequest};
use tokio::task::JoinHandle;

// Re-export types for public API
pub use common::ImageUrl;

// Generated from protos/game_host.proto
pub mod game_host {
    tonic::include_proto!("achtung.gamehost");
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

    /// Arena dimensions
    pub arena_width: u32,
    pub arena_height: u32,

    /// How long to wait between games
    pub game_interval: Duration,

    /// How often to poll game status
    pub poll_interval: Duration,

    /// gRPC port that the game host listens on
    pub game_host_grpc_port: u16,

    /// gRPC port that agents listen on
    pub agent_grpc_port: u16,
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
}

impl<P: MachineProvider> GameCoordinator<P> {
    pub fn new(
        config: CoordinatorConfig,
        machine_provider: Arc<P>,
        agent_repo: Box<dyn AgentRepository>,
        token_provider: Box<dyn DeployTokenProvider>,
    ) -> Self {
        Self {
            config,
            machine_provider,
            agent_repo,
            token_provider,
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
        let ctx = self
            .machine_provider
            .init_match(&match_id)
            .await
            .map_err(CoordinatorError::MachineSpawn)?;

        // 3. Run the game, then always clean up
        let game_result = self.run_game_inner(&ctx, &agents).await;

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
        agents: &[AgentInfo],
    ) -> Result<GameResult, CoordinatorError> {
        // Spawn game host (slot 0)
        let game_host_handle = self.spawn_game_host(ctx).await?;
        tracing::info!("Game host spawned at {}", game_host_handle.private_ip);

        // Spawn agents (slots 1+), cleaning up on failure
        let mut agent_handles: Vec<(AgentId, MachineHandle)> = Vec::new();
        for (i, agent) in agents.iter().enumerate() {
            let slot = (i + 1) as u8;
            match self.spawn_agent(ctx, agent, slot).await {
                Ok(handle) => {
                    tracing::info!(
                        agent_id = agent.id,
                        ip = handle.private_ip,
                        slot,
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

        // Run the game
        let game_result = self.run_game(&game_host_handle, &agent_handles).await;

        // Destroy machines regardless of game outcome
        self.destroy_all(ctx, Some(&game_host_handle), &agent_handles)
            .await;

        game_result
    }

    async fn spawn_game_host(
        &self,
        ctx: &P::MatchContext,
    ) -> Result<MachineHandle, CoordinatorError> {
        // Game host is on a public registry, no copy or token needed
        let config = SpawnConfig::new(
            ContainerImage::Public(self.config.game_host_image.clone()),
            0,
        )
        .env("NUM_PLAYERS", self.config.agents_per_game.to_string())
        .env("GAME", "achtung")
        .env("TICK_RATE_MS", self.config.tick_rate_ms.to_string());

        self.machine_provider
            .spawn(ctx, config)
            .await
            .map_err(CoordinatorError::MachineSpawn)
    }

    async fn spawn_agent(
        &self,
        ctx: &P::MatchContext,
        agent: &AgentInfo,
        slot: u8,
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

        let config = SpawnConfig::new(container_image, slot);

        self.machine_provider
            .spawn(ctx, config)
            .await
            .map_err(CoordinatorError::MachineSpawn)
    }

    async fn run_game(
        &self,
        game_host: &MachineHandle,
        agents: &[(AgentId, MachineHandle)],
    ) -> Result<GameResult, CoordinatorError> {
        // Wait for the game host to start up
        tokio::time::sleep(Duration::from_secs(5)).await;

        let game_host_addr = format!(
            "http://{}:{}",
            game_host.private_ip, self.config.game_host_grpc_port
        );

        let mut client = GameHostClient::connect(game_host_addr)
            .await
            .map_err(|e| CoordinatorError::Connection(e.to_string()))?;

        let agent_endpoints: Vec<AgentEndpoint> = agents
            .iter()
            .map(|(id, handle)| AgentEndpoint {
                agent_id: *id,
                address: format!("{}:{}", handle.private_ip, self.config.agent_grpc_port),
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

        let start_response = client
            .start_game(start_request)
            .await
            .map_err(|e| CoordinatorError::GameHost(e.to_string()))?;

        let game_id = start_response.into_inner().game_id;
        tracing::info!("Game started: {}", game_id);

        // Poll until the game ends
        loop {
            tokio::time::sleep(self.config.poll_interval).await;

            let status = client
                .get_status(GetStatusRequest {
                    game_id: game_id.clone(),
                })
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
        if let Some(handle) = game_host
            && let Err(e) = self.machine_provider.destroy(ctx, handle).await
        {
            tracing::error!("Failed to destroy game host: {}", e);
        }
        for (agent_id, handle) in agents {
            if let Err(e) = self.machine_provider.destroy(ctx, handle).await {
                tracing::error!("Failed to destroy agent {}: {}", agent_id, e);
            }
        }
    }
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
#[derive(Debug)]
pub enum CoordinatorError {
    Database(Box<dyn std::error::Error + Send + Sync>),
    MachineSpawn(MachineError),
    DeployToken(Box<dyn std::error::Error + Send + Sync>),
    Connection(String),
    GameHost(String),
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorError::Database(e) => write!(f, "Database error: {}", e),
            CoordinatorError::MachineSpawn(e) => write!(f, "Failed to spawn machine: {}", e),
            CoordinatorError::DeployToken(e) => write!(f, "Failed to get deploy token: {}", e),
            CoordinatorError::Connection(e) => write!(f, "Connection error: {}", e),
            CoordinatorError::GameHost(e) => write!(f, "Game host error: {}", e),
        }
    }
}

impl std::error::Error for CoordinatorError {}
