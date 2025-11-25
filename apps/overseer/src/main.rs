use std::env;
use tonic::transport::Server;
use tournament_mananger::tournament_manager_server::TournamentManagerServer;

pub mod tournament_mananger {
    tonic::include_proto!("achtung.tournament");
}
pub mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse().unwrap();

    // Get registry URL from environment or use default
    let registry_url =
        env::var("REGISTRY_URL").unwrap_or_else(|_| "https://achtung-registry.fly.dev".to_string());

    let overseer = server::Overseer::new(registry_url);

    println!("Overseer listening on {addr}");

    Server::builder()
        .add_service(TournamentManagerServer::new(overseer))
        .serve(addr)
        .await?;

    Ok(())
}
