use reqwest::Client;

#[derive(Debug, Clone)]
pub enum AgentStatus {
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

#[derive(serde::Serialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub git_repo: String,
    pub dockerfile_path: Option<String>,
    pub context_sub_path: Option<String>,
}

pub enum BuildServerRequest {
    CreateAgent(CreateAgentRequest),
}

pub struct AgentManager {
    client: Client,
    base_url: String,
}

impl AgentManager {
    pub fn new(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    async fn request(&self, req: BuildServerRequest) -> Result<(), reqwest::Error> {
        match req {
            BuildServerRequest::CreateAgent(create_agent) => {
                self.client
                    .post(&format!("{}/build-and-deploy", self.base_url))
                    .json(&create_agent)
                    .send()
                    .await?
                    .error_for_status()?;
            }
        }

        Ok(())
    }

    pub async fn create_agent(
        &self,
        create_agent: CreateAgentRequest,
    ) -> Result<(), reqwest::Error> {
        self.request(BuildServerRequest::CreateAgent(create_agent))
            .await
    }

    pub async fn get_agents(&self) -> Result<Vec<Agent>, reqwest::Error> {
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
                status: AgentStatus::Inactive,
                stats: AgentStats {
                    wins: 5,
                    losses: 10,
                    rank: 2,
                },
            },
            Agent {
                name: "Charlie".to_string(),
                status: AgentStatus::Active,
                stats: AgentStats {
                    wins: 7,
                    losses: 7,
                    rank: 3,
                },
            },
            Agent {
                name: "David".to_string(),
                status: AgentStatus::Active,
                stats: AgentStats {
                    wins: 6,
                    losses: 8,
                    rank: 4,
                },
            },
            Agent {
                name: "Eve".to_string(),
                status: AgentStatus::Inactive,
                stats: AgentStats {
                    wins: 4,
                    losses: 11,
                    rank: 5,
                },
            },
            Agent {
                name: "Frank".to_string(),
                status: AgentStatus::Active,
                stats: AgentStats {
                    wins: 8,
                    losses: 6,
                    rank: 6,
                },
            },
            Agent {
                name: "Grace".to_string(),
                status: AgentStatus::Active,
                stats: AgentStats {
                    wins: 9,
                    losses: 5,
                    rank: 7,
                },
            },
        ];

        Ok(agents)
    }
}
