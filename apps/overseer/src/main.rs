use tonic::transport::Server;
use tournament_mananger::tournament_manager_server::TournamentManagerServer;

pub mod tournament_mananger {
    tonic::include_proto!("achtung.tournament");
}
pub mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse().unwrap();
    let greeter = server::Overseer::default();

    println!("GreeterServer listening on {addr}");

    Server::builder()
        .add_service(TournamentManagerServer::new(greeter))
        .serve(addr)
        .await?;

    Ok(())
}
