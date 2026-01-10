use std::collections::HashMap;

use crate::fly_api::{self, FlyApi};
use crate::registry_client::RegistryClient;
use crate::tournament_mananger::tournament_manager_server::TournamentManager;
use crate::tournament_mananger::{
    AgentId, AgentImage, CreateAgentRequest, CreateAgentResponse, DeleteAgentRequest,
    DeleteAgentResponse, ListImagesRequest, ListImagesResponse, NewAgentVersionRequest,
    NewAgentVersionResponse, UpdateAgentStateRequest, UpdateAgentStateResponse,
};
use rand::{Rng, distr::Alphanumeric};
use rand::{RngCore, TryRngCore};
use reqwest::Client;
use tonic::{Request, Response, Status};

#[derive(Debug)]
pub struct OverseerConfig {
    fly_simlulation_org: String,
    fly_token: String,
    fly_host: crate::fly_api::FlyHost,
    registry_url: String,
}

impl OverseerConfig {
    pub fn new(
        fly_simlulation_org: String,
        fly_token: String,
        fly_host: crate::fly_api::FlyHost,
        registry_url: String,
    ) -> Self {
        Self {
            fly_simlulation_org,
            fly_token,
            fly_host,
            registry_url,
        }
    }
}

#[derive(Debug)]
pub struct Overseer {
    // TODO: Initialize db_pool properly
    // db_pool: PgPool,
    fly_api: FlyApi,
    registry_client: RegistryClient,
    config: OverseerConfig,
}

impl Overseer {
    pub fn new(config: OverseerConfig) -> Self {
        let http_client = Client::new();
        let fly_api = FlyApi::new(
            config.fly_token.clone(),
            http_client.clone(),
            config.fly_host.clone(),
        );
        let registry_client = RegistryClient::new(config.registry_url.clone(), http_client);
        Self {
            fly_api,
            registry_client,
            config,
        }
    }
}

fn generate_agent_id() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect::<String>()
        .to_lowercase()
}

#[tonic::async_trait]
impl TournamentManager for Overseer {
    async fn create_agent(
        &self,
        request: Request<CreateAgentRequest>,
    ) -> Result<Response<CreateAgentResponse>, Status> {
        let request = request.into_inner();
        let agent_id = generate_agent_id();
        let network = format!("agent-{agent_id}-network");
        let agent_app_name = format!("achtung-agent-{agent_id}-app");
        let org = self.config.fly_simlulation_org.clone();

        // Create Fly app for the agent
        self.fly_api
            .create_app(agent_app_name.clone(), org.clone(), network.clone())
            .await
            .map_err(|e| {
                tracing::warn!("Failed to create Fly app: {}", e);
                Status::internal(format!("Failed to create Fly app: {}", e))
            })?;

        // Assign private IPv6 to the app
        let service_name = "agent-service";
        self.fly_api
            .assign_ip(
                agent_app_name.clone(),
                network.clone(),
                org.clone(),
                service_name.into(),
                fly_api::FlyIpType::PrivateV6,
            )
            .await
            .map_err(|e| {
                tracing::warn!("Failed to assign IP to Fly app: {}", e);
                Status::internal(format!("Failed to assign IP to Fly app: {}", e))
            })?;

        // Copy image from registry to Fly's registry
        let credentials = request
            .registry_credentials
            .ok_or_else(|| Status::invalid_argument("registry_credentials are required"))?;
        let image = request
            .image
            .clone()
            .ok_or_else(|| Status::invalid_argument("image is required to create an agent"))?
            .image_url;
        let registry_host = self
            .config
            .registry_url
            .split_once("://")
            .map(|(_, host)| host)
            .unwrap_or(&self.config.registry_url);
        let source_image_url = format!("{}/{}", registry_host, image);
        let destination_image_url = format!("registry.fly.io/{}", agent_app_name);
        self.registry_client
            .copy_image(
                &source_image_url,
                &destination_image_url,
                &credentials.token,
                &crate::registry_client::BasicRegistryCredentials {
                    username: "x".into(),
                    password: self.config.fly_token.clone(),
                },
            )
            .await
            .map_err(|e| {
                tracing::warn!("Failed to copy image to Fly registry: {}", e);
                Status::internal(format!("Failed to copy image to Fly registry: {}", e))
            })?;

        // Create machine
        let app_config = fly_api::FlyMachineConfig {
            image: destination_image_url,
            env: HashMap::new(),
            auto_destroy: true,
            restart: fly_api::FlyRestartConfig {
                max_retries: 1,
                policy: fly_api::FlyRestartPolicy::OnFailure,
            },
        };
        self.fly_api
            .create_machine(agent_app_name, app_config)
            .await
            .map_err(|e| {
                tracing::warn!("Failed to create machine for Fly app: {}", e);
                Status::internal(format!("Failed to create machine for Fly app: {}", e))
            })?;

        tracing::info!("Created agent with ID: {}", agent_id);

        Ok(Response::new(CreateAgentResponse {
            agent_id: Some(AgentId { id: agent_id }),
        }))
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
        // test like a boss haha eks dee
        todo!()
    }

    async fn new_agent_version(
        &self,
        request: Request<NewAgentVersionRequest>,
    ) -> Result<Response<NewAgentVersionResponse>, Status> {
        Ok(Response::new(NewAgentVersionResponse {}))
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

        let namespace = format!("user-{}/", user_id);

        let images: Vec<AgentImage> = self
            .registry_client
            .list_images(&namespace, &credentials.token)
            .await
            .map_err(|e| Status::internal(format!("Failed to list images: {}", e)))?
            .into_iter()
            .map(|repo| AgentImage {
                image_url: repo.strip_prefix(&namespace).unwrap_or(&repo).to_string(),
            })
            .collect();

        let response = ListImagesResponse { images };
        Ok(Response::new(response))
    }
}
