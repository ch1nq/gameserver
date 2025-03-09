use crate::build_service::build_service_client::BuildServiceClient;
use crate::build_service::{BuildRequest, BuildResponse, PollBuildRequest, PollBuildResponse};

#[derive(Debug, Clone)]
pub enum AgentStatus {
    Created,
    Building { build_id: String },
    BuildFailed,
    Active,
    Inactive,
}

#[derive(Debug, Clone)]
pub struct AgentStats {
    pub wins: u32,
    pub losses: u32,
    pub rank: u32,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub name: String,
    pub status: AgentStatus,
    pub stats: AgentStats,
}

impl Agent {
    pub fn new(name: String) -> Self {
        Self {
            name,
            status: AgentStatus::Created,
            stats: AgentStats {
                wins: 0,
                losses: 0,
                rank: 0,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentManager {
    client: BuildServiceClient<tonic::transport::Channel>,
    agents: Vec<Agent>,
}

type AgentManagerError = Box<dyn std::error::Error>;

impl AgentManager {
    pub fn new(client: BuildServiceClient<tonic::transport::Channel>) -> Self {
        let agents = vec![
            Agent {
                name: "Alice".to_string(),
                status: AgentStatus::Active,
                stats: AgentStats {
                    wins: 10,
                    losses: 5,
                    rank: 1,
                },
            },
            Agent {
                name: "Bob".to_string(),
                status: AgentStatus::Created,
                stats: AgentStats {
                    wins: 5,
                    losses: 10,
                    rank: 2,
                },
            },
        ];
        Self { client, agents }
    }
    pub async fn create_agent(
        &mut self,
        name: String,
        git_repo: &str,
        dockerfile_path: Option<&str>,
        context_sub_path: Option<&str>,
    ) -> Result<(), AgentManagerError> {
        let mut agent = Agent::new(name);

        let build_id = "123".to_string();

        agent.status = AgentStatus::Building { build_id };
        self.agents.push(agent);

        Ok(())
    }

    pub async fn poll_build_status(&mut self) -> Result<(), AgentManagerError> {
        Ok(())
    }

    pub async fn get_agents(&self) -> Result<Vec<Agent>, reqwest::Error> {
        let agents = self.agents.clone();
        Ok(agents)
    }
}
