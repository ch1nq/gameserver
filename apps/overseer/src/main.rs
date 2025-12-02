use std::env;
use tonic::transport::Server;
use tournament_mananger::tournament_manager_server::TournamentManagerServer;

pub mod tournament_mananger {
    tonic::include_proto!("achtung.tournament");
}
pub mod fly_api;
pub mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = env::var("HOST").unwrap_or_else(|_| "[::]".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "50051".to_string());
    let registry_url =
        env::var("REGISTRY_URL").unwrap_or_else(|_| "https://achtung-registry.fly.dev".to_string());

    let addr = format!("{host}:{port}").parse().unwrap();
    let overseer = server::Overseer::new(registry_url);

    println!("Overseer listening on {addr}");

    Server::builder()
        .add_service(TournamentManagerServer::new(overseer))
        .serve(addr)
        .await?;

    Ok(())
}
