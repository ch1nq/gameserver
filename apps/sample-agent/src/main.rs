//! Sample gRPC agent (`achtung.agent`).
//!
//! Deliberately dumb: it ignores the game state entirely and picks a random
//! direction each tick (heavily weighted toward going straight so the game
//! still lasts more than a couple of ticks). This is a pipeline test agent, not
//! a competitor — the point is to exercise Initialize/GetAction and let the
//! engine resolve a placement order, not to play well.

use tonic::{transport::Server, Request, Response, Status};

pub mod agentpb {
    tonic::include_proto!("achtung.agent");
}

use agentpb::agent_server::{Agent, AgentServer};
use agentpb::{AgentAction, Direction, GameState, InitializeRequest, InitializeResponse};

struct SampleAgent;

#[tonic::async_trait]
impl Agent for SampleAgent {
    async fn initialize(
        &self,
        request: Request<InitializeRequest>,
    ) -> Result<Response<InitializeResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(player_id = req.player_id, "initialized");
        Ok(Response::new(InitializeResponse {}))
    }

    async fn get_action(
        &self,
        _request: Request<GameState>,
    ) -> Result<Response<AgentAction>, Status> {
        // Really dumb: random walk. Mostly straight, occasional random turn.
        let direction = match rand::random_range(0..10) {
            0 => Direction::TurnLeft,
            1 => Direction::TurnRight,
            _ => Direction::Staight,
        };
        Ok(Response::new(AgentAction {
            direction: direction as i32,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sample_agent=info,info".into()),
        )
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50052);
    let addr = format!("0.0.0.0:{port}").parse()?;

    let svc = SampleAgent;

    tracing::info!(%addr, "sample-agent listening");
    Server::builder()
        .add_service(AgentServer::new(svc))
        .serve(addr)
        .await?;
    Ok(())
}
