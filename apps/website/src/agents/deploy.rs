use crate::agents::agent;

// Generated proto bindings
pub mod agent_deploy_service {
    tonic::include_proto!("deployagent");
}

pub trait AgentDeployer {
    type Error;

    async fn deploy_agent(
        &self,
        agent_id: agent::AgentId,
        image_url: agent::ImageUrl,
    ) -> Result<(), Self::Error>;

    async fn delete_agent(&self, agent_id: agent::AgentId) -> Result<(), Self::Error>;
}

pub struct AgentDeployService {
    agent_deploy_service_url: String,
    user_id: String,
}

impl AgentDeployService {
    pub fn new(agent_deploy_service_url: String, user_id: String) -> Self {
        Self {
            agent_deploy_service_url,
            user_id,
        }
    }
}

#[derive(Debug)]
pub enum AgentDeployerError {
    GrpcError(tonic::Status),
    ConnectionError(String),
    DeploymentFailed(String),
}

impl std::fmt::Display for AgentDeployerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GrpcError(status) => write!(f, "gRPC error: {}", status),
            Self::ConnectionError(e) => write!(f, "Connection error: {}", e),
            Self::DeploymentFailed(e) => write!(f, "Deployment failed: {}", e),
        }
    }
}

impl std::error::Error for AgentDeployerError {}

impl From<tonic::transport::Error> for AgentDeployerError {
    fn from(e: tonic::transport::Error) -> Self {
        Self::ConnectionError(e.to_string())
    }
}

impl From<tonic::Status> for AgentDeployerError {
    fn from(e: tonic::Status) -> Self {
        Self::GrpcError(e)
    }
}

impl AgentDeployer for AgentDeployService {
    type Error = AgentDeployerError;

    async fn deploy_agent(
        &self,
        agent_id: agent::AgentId,
        image_url: agent::ImageUrl,
    ) -> Result<(), Self::Error> {
        use agent_deploy_service::agent_deploy_service_client::AgentDeployServiceClient;

        tracing::info!(
            agent_id = agent_id,
            image_url = image_url.as_ref(),
            "Deploying agent via AgentDeployService"
        );

        let mut client =
            AgentDeployServiceClient::connect(self.agent_deploy_service_url.clone()).await?;

        let request = tonic::Request::new(agent_deploy_service::DeployAgentRequest {
            name: format!("agent-{}", agent_id),
            image_url: image_url.to_string(),
            agent_id,
            registry_credentials: None,
        });

        // Add user-id metadata
        let mut request = request;
        request
            .metadata_mut()
            .insert("user-id", self.user_id.parse().unwrap());

        let response = client.deploy_agent(request).await?;

        let response = response.into_inner();

        if response.status() == agent_deploy_service::deploy_agent_response::Status::Error {
            return Err(AgentDeployerError::DeploymentFailed(response.message));
        }

        tracing::info!(
            agent_id = agent_id,
            app_name = response.app_name,
            deployed_image = response.deployed_image_url,
            "Successfully deployed agent"
        );

        Ok(())
    }

    async fn delete_agent(&self, agent_id: agent::AgentId) -> Result<(), Self::Error> {
        use agent_deploy_service::agent_deploy_service_client::AgentDeployServiceClient;

        tracing::info!(agent_id = agent_id, "Deleting agent via AgentDeployService");

        let mut client =
            AgentDeployServiceClient::connect(self.agent_deploy_service_url.clone()).await?;

        let request = tonic::Request::new(agent_deploy_service::DeleteAgentRequest {
            name: format!("agent-{}", agent_id),
            agent_id,
        });

        // Add user-id metadata
        let mut request = request;
        request
            .metadata_mut()
            .insert("user-id", self.user_id.parse().unwrap());

        let response = client.delete_agent(request).await?;

        let response = response.into_inner();

        if response.status() == agent_deploy_service::delete_agent_response::Status::Error {
            return Err(AgentDeployerError::DeploymentFailed(response.message));
        }

        tracing::info!(agent_id = agent_id, "Successfully deleted agent");

        Ok(())
    }
}
