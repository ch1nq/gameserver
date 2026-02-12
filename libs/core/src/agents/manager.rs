use crate::agents::agent::{Agent, AgentId, AgentImageUrl, AgentName, AgentStatus};
use crate::users::UserId;
use common::{AgentInfo, AgentRepository, ContainerImageUrl};
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct AgentManager {
    db_pool: PgPool,
}

type AgentManagerError = Box<dyn std::error::Error>;

impl AgentManager {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn create_agent(
        &self,
        name: AgentName,
        user_id: UserId,
        image_url: AgentImageUrl,
    ) -> Result<Agent, AgentManagerError> {
        let agent_id = sqlx::query!(
            r#"
            INSERT INTO agents (name, status, user_id, image_url)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
            &*name,
            AgentStatus::Inactive as AgentStatus,
            user_id,
            image_url.as_url(),
        )
        .fetch_one(&self.db_pool)
        .await?
        .id;

        tracing::info!(agent_id = agent_id, "Created new agent");

        Ok(Agent {
            id: agent_id,
            name,
            user_id,
            status: AgentStatus::Inactive,
            image_url,
        })
    }

    pub async fn activate_agent(
        &self,
        agent_id: AgentId,
        user_id: UserId,
    ) -> Result<Agent, AgentManagerError> {
        let agent = sqlx::query_as::<_, Agent>(
            r#"
            UPDATE agents
            SET status = $1
            WHERE id = $2 AND user_id = $3 AND image_url IS NOT NULL
            RETURNING id, name, user_id, status, image_url
            "#,
        )
        .bind(AgentStatus::Active)
        .bind(agent_id)
        .bind(user_id)
        .fetch_one(&self.db_pool)
        .await?;

        tracing::info!(agent_id = agent_id, "Activated agent");

        Ok(agent)
    }

    pub async fn deactivate_agent(
        &self,
        agent_id: AgentId,
        user_id: UserId,
    ) -> Result<Agent, AgentManagerError> {
        let agent = sqlx::query_as::<_, Agent>(
            r#"
            UPDATE agents
            SET status = $1
            WHERE id = $2 AND user_id = $3
            RETURNING id, name, user_id, status, image_url
            "#,
        )
        .bind(AgentStatus::Inactive)
        .bind(agent_id)
        .bind(user_id)
        .fetch_one(&self.db_pool)
        .await?;

        tracing::info!(agent_id = agent_id, "Deactivated agent");

        Ok(agent)
    }

    pub async fn get_agents_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<Agent>, AgentManagerError> {
        let agents = sqlx::query_as::<_, Agent>(
            r#"
            SELECT id, name, user_id, status, image_url
            FROM agents
            WHERE user_id = $1
            ORDER BY id DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db_pool)
        .await?;
        Ok(agents)
    }

    pub async fn get_agents(&self) -> Result<Vec<Agent>, AgentManagerError> {
        let agents = sqlx::query_as::<_, Agent>(
            r#"
            SELECT id, name, user_id, status, image_url
            FROM agents
            ORDER BY id DESC
            "#,
        )
        .fetch_all(&self.db_pool)
        .await?;
        Ok(agents)
    }

    pub async fn delete_agent(
        &self,
        agent_id: AgentId,
        user_id: UserId,
    ) -> Result<(), AgentManagerError> {
        sqlx::query!(
            r#"
            DELETE FROM agents
            WHERE id = $1 AND user_id = $2
            "#,
            agent_id,
            user_id,
        )
        .execute(&self.db_pool)
        .await?;

        tracing::info!(agent_id = agent_id, "Deleted agent");

        Ok(())
    }

    /// Get N random active agents for a match
    pub async fn get_random_active_agents(
        &self,
        count: usize,
    ) -> Result<Vec<AgentInfo>, sqlx::Error> {
        let agents = sqlx::query_as::<_, (i64, i64, String)>(
            r#"
            SELECT id, user_id, image_url
            FROM agents
            WHERE status = 'active'
            ORDER BY RANDOM()
            LIMIT $1
            "#,
        )
        .bind(count as i64)
        .fetch_all(&self.db_pool)
        .await?;

        Ok(agents
            .into_iter()
            .map(|(id, user_id, image_url_str)| {
                // Parse image URL - should always succeed since we validated on creation
                let image_url =
                    AgentImageUrl::parse_full(&image_url_str, user_id).unwrap_or_else(|e| {
                        tracing::error!(
                            agent_id = id,
                            error = %e,
                            "Failed to parse agent image URL from database"
                        );
                        panic!("Invalid agent image in database: {}", e);
                    });

                AgentInfo { id, image_url }
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl AgentRepository for AgentManager {
    async fn get_random_active_agents(
        &self,
        count: usize,
    ) -> Result<Vec<AgentInfo>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.get_random_active_agents(count).await?)
    }
}
