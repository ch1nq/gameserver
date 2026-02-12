use crate::{AgentId, AgentImageUrl, ContainerImageUrl, RegistryToken};

/// Agent info needed for a match
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: AgentId,
    pub image_url: AgentImageUrl,
}

/// Trait for fetching active agents from the database
#[async_trait::async_trait]
pub trait AgentRepository: Send + Sync {
    /// Get N random active agents for a match
    async fn get_random_active_agents(
        &self,
        count: usize,
    ) -> Result<Vec<AgentInfo>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Trait for generating scoped deploy tokens for pulling images from the registry
#[async_trait::async_trait]
pub trait DeployTokenProvider: Send + Sync {
    /// Get a short-lived token with pull access to the given container image
    ///
    /// Accepts any type implementing `ContainerImageUrl` (e.g., `AgentImageUrl`, `ImageUrl`).
    /// The repository is extracted from the image URL internally via the trait method.
    async fn get_deploy_token(
        &self,
        image: &(dyn ContainerImageUrl + Send + Sync),
    ) -> Result<RegistryToken, Box<dyn std::error::Error + Send + Sync>>;
}
