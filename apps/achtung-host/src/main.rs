//! Thin Achtung gRPC game-host binary.
//!
//! All orchestration lives in `arcadio`'s generic [`GrpcGameServer`]; this just
//! wires up the Achtung adapter and serves it. `PORT` selects the listen port
//! (default 50051); arena dimensions come from `ARENA_WIDTH`/`ARENA_HEIGHT`.

use arcadio::games::achtung_grpc::AchtungGrpc;
use arcadio::grpc::GrpcGameServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "achtung_host=info,arcadio=info,info".into()),
        )
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50051);

    GrpcGameServer::new(AchtungGrpc::from_env())
        .serve(port)
        .await
}
