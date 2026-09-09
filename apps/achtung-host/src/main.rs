//! Thin Achtung gRPC game-host binary.
//!
//! All orchestration lives in `arcadio`'s generic [`GrpcGameServer`]; this just
//! wires up the Achtung adapter and serves it. Startup config comes from one
//! typed [`GameHostConfig::from_env`] parse (fail-fast, single error); arena
//! dimensions share their *names* with the website via `env_names` (#11).

use achtung_config::GameHostConfig;
use arcadio::games::achtung::AchtungConfig;
use arcadio::games::achtung_grpc::AchtungGrpc;
use arcadio::grpc::GrpcGameServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match GameHostConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(1);
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.rust_log.clone().into()),
        )
        .init();

    let adapter = AchtungGrpc::new(AchtungConfig {
        arena_width: config.arena_width,
        arena_height: config.arena_height,
        edge_wrapping: false,
    });
    GrpcGameServer::new(adapter).serve(config.port).await
}
