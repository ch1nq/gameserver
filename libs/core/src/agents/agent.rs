use sqlx::Row;

pub use common::{AgentId, AgentImageUrl, AgentName, AgentStatus, UserId};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Agent {
    pub id: AgentId,
    pub name: AgentName,
    pub user_id: UserId,
    pub status: AgentStatus,
    pub image_url: AgentImageUrl,
}

// Custom FromRow implementation since AgentImageUrl needs parsing
impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for Agent {
    fn from_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let id: AgentId = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let user_id: UserId = row.try_get("user_id")?;
        let status: AgentStatus = row.try_get("status")?;
        let image_url_str: String = row.try_get("image_url")?;

        // Parse image URL - should always succeed since we validated on creation
        let image_url = AgentImageUrl::parse_full(&image_url_str, user_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        Ok(Agent {
            id,
            name: AgentName::from(name),
            user_id,
            status,
            image_url,
        })
    }
}

impl From<Agent> for api_types::Agent {
    fn from(a: Agent) -> Self {
        Self {
            id: a.id,
            name: a.name,
            user_id: a.user_id,
            status: a.status,
            image_url: a.image_url,
        }
    }
}
