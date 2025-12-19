use std::env;
use tonic::transport::Server;
use tournament_mananger::tournament_manager_server::TournamentManagerServer;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub mod tournament_mananger {
    tonic::include_proto!("achtung.tournament");
}
pub mod fly_api;
pub mod registry_client;
pub mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry().with(EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else( |_| {
            "overseer=debug,sqlx=warn,tonic=debug,reqwest=debug,fly_api=debug,registry_client=debug"
                .into()
        }
        )))
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    let host = env::var("HOST").unwrap_or_else(|_| "[::]".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "50051".to_string());
    let addr = format!("{host}:{port}").parse().unwrap();

    let registry_url =
        env::var("REGISTRY_URL").unwrap_or_else(|_| "https://achtung-registry.fly.dev".to_string());
    let fly_token = env::var("FLY_TOKEN").expect("FLY_TOKEN must be set");
    let fly_simlulation_org =
        env::var("FLY_SIMULATION_ORG").expect("FLY_SIMULATION_ORG must be set");
    let fly_host = match env::var("FLY_HOST")
        .unwrap_or_else(|_| "public".to_string())
        .as_str()
    {
        "internal" => fly_api::FlyHost::Internal,
        _ => fly_api::FlyHost::Public,
    };
    let config =
        server::OverseerConfig::new(fly_simlulation_org, fly_token, fly_host, registry_url);

    let overseer = server::Overseer::new(config);

    println!("Overseer listening on {addr}");

    Server::builder()
        .add_service(TournamentManagerServer::new(overseer))
        .serve(addr)
        .await?;

    Ok(())
}
