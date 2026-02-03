use sqlx::FromRow;

pub use common::{AgentId, AgentName, AgentStatus, ImageUrl, UserId};

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Agent {
    pub id: AgentId,
    pub name: AgentName,
    pub user_id: UserId,
    pub status: AgentStatus,
    pub image_url: ImageUrl,
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
