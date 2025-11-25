use reqwest::Client;
use serde::Deserialize;
use tonic::{Request, Response, Status};

use crate::tournament_mananger::tournament_manager_server::TournamentManager;
use crate::tournament_mananger::{
    AgentImage, CreateAgentRequest, CreateAgentResponse, DeleteAgentRequest, DeleteAgentResponse,
    ListImagesRequest, ListImagesResponse, NewAgentVersionRequest, NewAgentVersionResponse,
    UpdateAgentStateRequest, UpdateAgentStateResponse,
};

#[derive(Deserialize)]
struct CatalogResponse {
    repositories: Vec<String>,
}

pub struct Overseer {
    // TODO: Initialize db_pool properly
    // db_pool: PgPool,
    http_client: Client,
    registry_url: String,
}

impl Overseer {
    pub fn new(registry_url: String) -> Self {
        Self {
            http_client: Client::new(),
            registry_url,
        }
    }
}

#[tonic::async_trait]
impl TournamentManager for Overseer {
    async fn create_agent(
        &self,
        request: Request<CreateAgentRequest>,
    ) -> Result<Response<CreateAgentResponse>, Status> {
        todo!()
    }

    async fn delete_agent(
        &self,
        request: Request<DeleteAgentRequest>,
    ) -> Result<Response<DeleteAgentResponse>, Status> {
        todo!()
    }

    async fn update_agent_state(
        &self,
        request: Request<UpdateAgentStateRequest>,
    ) -> Result<Response<UpdateAgentStateResponse>, Status> {
        todo!()
    }

    async fn new_agent_version(
        &self,
        request: Request<NewAgentVersionRequest>,
    ) -> Result<Response<NewAgentVersionResponse>, Status> {
        todo!()
    }

    async fn list_images(
        &self,
        request: Request<ListImagesRequest>,
    ) -> Result<Response<ListImagesResponse>, Status> {
        let req = request.into_inner();

        let user_id = req
            .user_id
            .ok_or_else(|| Status::invalid_argument("user_id is required"))?
            .id;

        let credentials = req
            .registry_credentials
            .ok_or_else(|| Status::invalid_argument("registry_credentials are required"))?;

        // Fetch catalog from registry
        let catalog_url = format!("{}/v2/_catalog", self.registry_url);
        let response = self
            .http_client
            .get(&catalog_url)
            .bearer_auth(&credentials.token)
            .send()
            .await
            .map_err(|e| Status::internal(format!("Failed to connect to registry: {}", e)))?;

        if !response.status().is_success() {
            return Err(Status::internal(format!(
                "Registry returned error: {}",
                response.status()
            )));
        }

        let catalog: CatalogResponse = response
            .json()
            .await
            .map_err(|e| Status::internal(format!("Failed to parse registry response: {}", e)))?;

        // Filter repositories for this user's namespace: "user-{id}/*"
        let user_prefix = format!("user-{}/", user_id);
        let images: Vec<AgentImage> = catalog
            .repositories
            .into_iter()
            .filter(|repo| repo.starts_with(&user_prefix))
            .map(|repo| AgentImage {
                image_url: format!("{}/{}", self.registry_url, repo),
            })
            .collect();

        let response = ListImagesResponse { images };
        Ok(Response::new(response))
    }
}
