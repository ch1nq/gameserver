use crate::agents::agent::{Agent, AgentId, AgentImageUrl, AgentName, AgentStatus};
use crate::users::{UserId, Username};
use common::{AgentInfo, AgentRepository, ContainerImageUrl};
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct AgentManager {
    db_pool: PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentManagerError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// An agent plus its owner's username, for list pages that show `@author`.
#[derive(Debug, Clone)]
pub struct AgentWithAuthor {
    pub agent: Agent,
    pub username: Username,
}

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

    /// All agents joined with their owner's username, newest first.
    /// Used by the landing leaderboard to render `@author` + avatar.
    pub async fn get_agents_with_authors(&self) -> Result<Vec<AgentWithAuthor>, AgentManagerError> {
        let rows = sqlx::query(
            r#"
            SELECT a.id, a.name, a.user_id, a.status, a.image_url, u.username
            FROM agents a
            JOIN users u ON u.id = a.user_id
            ORDER BY a.id DESC
            "#,
        )
        .fetch_all(&self.db_pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: AgentId = row.try_get("id")?;
            let name: String = row.try_get("name")?;
            let user_id: UserId = row.try_get("user_id")?;
            let status: AgentStatus = row.try_get("status")?;
            let image_url_str: String = row.try_get("image_url")?;
            let username: Username = row.try_get("username")?;
            let image_url = AgentImageUrl::parse_full(&image_url_str, user_id)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            out.push(AgentWithAuthor {
                agent: Agent {
                    id,
                    name: AgentName::from(name),
                    user_id,
                    status,
                    image_url,
                },
                username,
            });
        }
        Ok(out)
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
    ) -> Result<Vec<AgentInfo>, AgentManagerError> {
        let agents = sqlx::query_as::<_, (i64, i64, String, String)>(
            r#"
            SELECT id, user_id, name, image_url
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
            .map(|(id, user_id, name, image_url_str)| {
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

                AgentInfo {
                    id,
                    name,
                    image_url,
                }
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
