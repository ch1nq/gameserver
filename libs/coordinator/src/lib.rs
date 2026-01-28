use std::time::Duration;

use agent_infra::{FlyMachineProvider, MachineError, MachineHandle, MachineProvider, SpawnConfig};
use game_host::game_host_client::GameHostClient;
use game_host::{AgentEndpoint, GameConfig, GameState, GetStatusRequest, StartGameRequest};
use tokio::task::JoinHandle;

// Generated from protos/game_host.proto
pub mod game_host {
    tonic::include_proto!("achtung.gamehost");
}

/// Agent info needed for a match
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: i64,
    pub image_url: String,
}

/// Trait for fetching active agents from the database
#[trait_variant::make(AgentRepository: Send)]
pub trait LocalAgentRepository {
    /// Get N random active agents for a match
    async fn get_random_active_agents(&self, count: usize) -> Result<Vec<AgentInfo>, sqlx::Error>;
}

/// Configuration for the game coordinator
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Machine provider configuration
    pub machine_provider: agent_infra::FlyMachineProviderConfig,

    /// Image URL for the game host container
    pub game_host_image: String,

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

/// The game coordinator that orchestrates matches
pub struct GameCoordinator<R: AgentRepository> {
    config: CoordinatorConfig,
    machine_provider: FlyMachineProvider,
    agent_repo: R,
}

impl<R: AgentRepository + Clone + Send + Sync + 'static> GameCoordinator<R> {
    pub fn new(config: CoordinatorConfig, agent_repo: R) -> Self {
        let machine_provider = FlyMachineProvider::new(config.machine_provider.clone());
        Self {
            config,
            machine_provider,
            agent_repo,
        }
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

            // Wait before starting next game
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

        // 2. Spawn game host machine
        let game_host_handle = self.spawn_game_host().await?;
        tracing::info!("Game host spawned: {}", game_host_handle.app_name);

        // 3. Spawn agent machines
        let mut agent_handles = Vec::new();
        for agent in &agents {
            match self.spawn_agent(agent).await {
                Ok(handle) => {
                    tracing::info!(
                        "Agent {} spawned: {} at {}",
                        agent.id,
                        handle.app_name,
                        handle.private_ip
                    );
                    agent_handles.push((agent.id, handle));
                }
                Err(e) => {
                    tracing::error!("Failed to spawn agent {}: {}", agent.id, e);
                    // Cleanup already-spawned machines
                    self.cleanup(&Some(game_host_handle), &agent_handles).await;
                    return Err(e);
                }
            }
        }

        // 4. Connect to game host and start game
        let game_result = self.run_game(&game_host_handle, &agent_handles).await;

        // 5. Cleanup all machines
        self.cleanup(&Some(game_host_handle), &agent_handles).await;

        // 6. Handle result
        match game_result {
            Ok(result) => {
                tracing::info!("Game finished: {:?}", result);
                // TODO: Record results in database
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async fn spawn_game_host(&self) -> Result<MachineHandle, CoordinatorError> {
        let config = SpawnConfig {
            image_url: self.config.game_host_image.clone(),
            registry_token: String::new(), // Game host image is public or pre-deployed
            env: std::collections::HashMap::new(),
        };

        self.machine_provider
            .spawn(config)
            .await
            .map_err(CoordinatorError::MachineSpawn)
    }

    async fn spawn_agent(&self, agent: &AgentInfo) -> Result<MachineHandle, CoordinatorError> {
        // TODO: Get registry token for this agent's image
        let config = SpawnConfig {
            image_url: agent.image_url.clone(),
            registry_token: String::new(), // TODO: Get actual token
            env: std::collections::HashMap::new(),
        };

        self.machine_provider
            .spawn(config)
            .await
            .map_err(CoordinatorError::MachineSpawn)
    }

    async fn run_game(
        &self,
        game_host: &MachineHandle,
        agents: &[(i64, MachineHandle)],
    ) -> Result<GameResult, CoordinatorError> {
        // Wait a bit for the game host to start
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Connect to game host
        let game_host_addr = format!(
            "http://[{}]:{}",
            game_host.private_ip, self.config.game_host_grpc_port
        );

        let mut client = GameHostClient::connect(game_host_addr)
            .await
            .map_err(|e| CoordinatorError::Connection(e.to_string()))?;

        // Build agent endpoints
        let agent_endpoints: Vec<AgentEndpoint> = agents
            .iter()
            .map(|(id, handle)| AgentEndpoint {
                agent_id: *id,
                address: format!("[{}]:{}", handle.private_ip, self.config.agent_grpc_port),
            })
            .collect();

        // Start the game
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
        tracing::info!("Game started with ID: {}", game_id);

        // Poll for completion
        loop {
            tokio::time::sleep(self.config.poll_interval).await;

            let status_request = GetStatusRequest {
                game_id: game_id.clone(),
            };

            let status = client
                .get_status(status_request)
                .await
                .map_err(|e| CoordinatorError::GameHost(e.to_string()))?
                .into_inner();

            match status.state() {
                GameState::Running => {
                    tracing::debug!("Game running, tick {}", status.current_tick);
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
                GameState::WaitingForAgents => {
                    tracing::debug!("Waiting for agents to connect...");
                }
                GameState::Unspecified => {
                    return Err(CoordinatorError::GameHost("Unknown game state".into()));
                }
            }
        }
    }

    async fn cleanup(&self, game_host: &Option<MachineHandle>, agents: &[(i64, MachineHandle)]) {
        // Destroy game host
        if let Some(handle) = game_host {
            if let Err(e) = self.machine_provider.destroy(handle).await {
                tracing::error!("Failed to destroy game host: {}", e);
            }
        }

        // Destroy agent machines
        for (agent_id, handle) in agents {
            if let Err(e) = self.machine_provider.destroy(handle).await {
                tracing::error!("Failed to destroy agent {}: {}", agent_id, e);
            }
        }
    }
}

/// Result of a completed game
#[derive(Debug)]
pub struct GameResult {
    pub winner_agent_id: Option<i64>,
    pub placements: Vec<AgentPlacement>,
}

#[derive(Debug)]
pub struct AgentPlacement {
    pub agent_id: i64,
    pub position: u32,
    pub score: u32,
}

/// Errors that can occur during coordination
#[derive(Debug)]
pub enum CoordinatorError {
    Database(sqlx::Error),
    MachineSpawn(MachineError),
    Connection(String),
    GameHost(String),
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorError::Database(e) => write!(f, "Database error: {}", e),
            CoordinatorError::MachineSpawn(e) => write!(f, "Failed to spawn machine: {}", e),
            CoordinatorError::Connection(e) => write!(f, "Connection error: {}", e),
            CoordinatorError::GameHost(e) => write!(f, "Game host error: {}", e),
        }
    }
}

impl std::error::Error for CoordinatorError {}
