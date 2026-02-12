#[cfg(feature = "client")]
pub mod client;
pub mod routes;

use common::{AgentId, AgentImageUrl, AgentName, AgentStatus, ApiTokenId, UserId};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub image: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub name: AgentName,
    pub user_id: UserId,
    pub status: AgentStatus,
    pub image_url: AgentImageUrl,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiToken {
    pub id: ApiTokenId,
    pub user_id: UserId,
    pub name: String,
    pub created_at: time::PrimitiveDateTime,
    pub revoked_at: Option<time::PrimitiveDateTime>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateTokenResponse {
    pub token: String,
}

#[derive(Debug, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum ApiError {
    #[error("Unauthorized")]
    Unauthorized,

    #[error("Not found")]
    NotFound,

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

#[trait_variant::make(Send)]
pub trait GameApi {
    async fn list_agents(&self) -> Result<Vec<Agent>, ApiError>;
    async fn create_agent(&self, req: CreateAgentRequest) -> Result<Agent, ApiError>;
    async fn activate_agent(&self, id: AgentId) -> Result<Agent, ApiError>;
    async fn deactivate_agent(&self, id: AgentId) -> Result<Agent, ApiError>;
    async fn delete_agent(&self, id: AgentId) -> Result<(), ApiError>;
    async fn list_images(&self) -> Result<Vec<AgentImageUrl>, ApiError>;
    async fn validate_image(&self, image: &str) -> Result<AgentImageUrl, ApiError>;
    async fn list_tokens(&self) -> Result<Vec<ApiToken>, ApiError>;
    async fn create_token(&self, req: CreateTokenRequest) -> Result<CreateTokenResponse, ApiError>;
    async fn revoke_token(&self, id: ApiTokenId) -> Result<(), ApiError>;
}
