use tonic::{Request, Response, Status};

use crate::tournament_mananger::tournament_manager_server::TournamentManager;
use crate::tournament_mananger::{
    CreateAgentRequest, CreateAgentResponse, DeleteAgentRequest, DeleteAgentResponse,
    ListImagesRequest, ListImagesResponse, NewAgentVersionRequest, NewAgentVersionResponse,
    UpdateAgentStateRequest, UpdateAgentStateResponse,
};

#[derive(Default)]
pub struct Overseer {
    // TODO: Initialize db_pool properly
    // db_pool: PgPool,
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
        todo!()
    }
}
