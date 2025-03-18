use crate::build_service::build_service_client::BuildServiceClient;
use crate::build_service::{
    self, build_response, poll_build_response, BuildRequest, BuildResponse, PollBuildRequest,
    PollBuildResponse,
};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, sqlx::Type, serde::Deserialize, serde::Serialize)]
#[sqlx(type_name = "agent_status", rename_all = "snake_case")]
pub enum AgentStatus {
    Created,
    Building,
    BuildFailed,
    Active,
    Inactive,
}

impl From<std::string::String> for AgentStatus {
    fn from(s: std::string::String) -> Self {
        match s.as_str() {
            "created" => AgentStatus::Created,
            "building" => AgentStatus::Building,
            "build_failed" => AgentStatus::BuildFailed,
            "active" => AgentStatus::Active,
            "inactive" => AgentStatus::Inactive,
            _ => panic!("Invalid agent status"),
        }
    }
}

type AgentId = i64;

#[derive(Debug, Clone, FromRow)]
pub struct Agent {
    id: AgentId,
    pub name: String,
    pub user_id: crate::users::UserId,
    pub status: AgentStatus,
    pub build_id: Option<String>,
}

impl Agent {
    pub fn new(id: AgentId, user_id: crate::users::UserId, name: String) -> Self {
        Self {
            id,
            name,
            user_id,
            status: AgentStatus::Created,
            build_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentManager {
    build_service_client: BuildServiceClient<tonic::transport::Channel>,
    db_pool: PgPool,
}

type AgentManagerError = Box<dyn std::error::Error>;

impl AgentManager {
    pub fn new(
        build_service_client: BuildServiceClient<tonic::transport::Channel>,
        db_pool: PgPool,
    ) -> Self {
        let build_service_client_2 = build_service_client.clone();
        let db_pool_2 = db_pool.clone();
        tokio::spawn(poll_build_status(build_service_client_2, db_pool_2));

        Self {
            build_service_client,
            db_pool,
        }
    }

    pub async fn create_agent(
        &mut self,
        name: String,
        user_id: crate::users::UserId,
        git_repo: String,
        dockerfile_path: Option<String>,
        context_sub_path: Option<String>,
    ) -> Result<Agent, AgentManagerError> {
        let response = self
            .build_service_client
            .build(BuildRequest {
                name: name.clone(),
                git_repo,
                dockerfile_path: dockerfile_path.unwrap_or("Dockerfile".to_string()),
                context_sub_path: context_sub_path.unwrap_or(".".to_string()),
            })
            .await?
            .into_inner();

        let status = match build_response::Status::try_from(response.status)? {
            build_response::Status::Success => AgentStatus::Building,
            build_response::Status::Error => AgentStatus::BuildFailed,
        };

        let mut agent = Agent {
            id: 0,
            name,
            user_id,
            status,
            build_id: Some(response.build_id),
        };

        agent.id = self.save_agent(&agent).await?;

        Ok(agent)
    }

    // Save an agent to the database
    async fn save_agent(&self, agent: &Agent) -> Result<AgentId, AgentManagerError> {
        let id = sqlx::query!(
            r#"
            INSERT INTO agents (name, status, user_id, build_id)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
            agent.name,
            agent.status.clone() as AgentStatus,
            agent.user_id,
            agent.build_id,
        )
        .fetch_one(&self.db_pool)
        .await?
        .id;
        Ok(id)
    }

    // Get a all agents for owned by a user
    pub async fn get_agents_for_user(
        &self,
        user_id: crate::users::UserId,
    ) -> Result<Vec<Agent>, AgentManagerError> {
        let agents = sqlx::query_as!(
            Agent,
            r#"
            SELECT * FROM agents
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_all(&self.db_pool)
        .await?;
        Ok(agents)
    }

    pub async fn get_agents(&self) -> Result<Vec<Agent>, AgentManagerError> {
        let agents = sqlx::query_as!(
            Agent,
            r#"
            SELECT * FROM agents
            "#,
        )
        .fetch_all(&self.db_pool)
        .await?;
        Ok(agents)
    }
}

/// Poll the build service for the status of all agents that are currently building
async fn poll_build_status(
    mut build_service_client: BuildServiceClient<tonic::transport::Channel>,
    db_pool: PgPool,
) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        let building_agents =
            sqlx::query_as!(Agent, r#"SELECT * FROM agents WHERE status = 'building'"#,)
                .fetch_all(&db_pool)
                .await
                .unwrap();

        for agent in building_agents {
            let poll_response = build_service_client
                .poll_build(PollBuildRequest {
                    build_id: agent.build_id.unwrap(),
                })
                .await
                .unwrap()
                .into_inner();

            if let Err(e) = poll_build_response::Status::try_from(poll_response.status) {
                tracing::error!("Error polling build status for agent {}: {}", agent.id, e);
                continue;
            }

            let build_status =
                match poll_build_response::BuildStatus::try_from(poll_response.build_status) {
                    Ok(poll_build_response::BuildStatus::Running) => AgentStatus::Building,
                    Ok(poll_build_response::BuildStatus::Failed) => AgentStatus::BuildFailed,
                    Ok(poll_build_response::BuildStatus::Succeeded) => AgentStatus::Active,
                    Ok(poll_build_response::BuildStatus::Unknown) => {
                        tracing::error!("Unknown build status for agent {}", agent.id);
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("Error polling build status for agent {}: {}", agent.id, e);
                        continue;
                    }
                };

            sqlx::query!(
                r#"UPDATE agents SET status = $1 WHERE id = $2"#,
                build_status.clone() as AgentStatus,
                agent.id
            )
            .execute(&db_pool)
            .await
            .unwrap();
        }
    }
}
