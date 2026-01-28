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
