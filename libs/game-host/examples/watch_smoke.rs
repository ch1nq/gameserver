//! Local smoke test for the spectator stream (no infra needed).
//!
//! Run a game host + two sample agents, then:
//!   cargo run -p arcadio --example watch_smoke
//! It calls StartGame, opens WatchGame, and prints the first snapshot + a few
//! deltas so you can confirm the stream is producing state.

use arcadio::games::achtung_grpc::spectpb;
use arcadio::grpc::gamehost::game_host_client::GameHostClient;
use arcadio::grpc::gamehost::{AgentEndpoint, GameConfig, StartGameRequest, WatchGameRequest};
use prost::Message as _;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = std::env::var("HOST").unwrap_or_else(|_| "http://127.0.0.1:50051".into());
    let mut client = GameHostClient::connect(host).await?;

    client
        .start_game(StartGameRequest {
            agents: vec![
                AgentEndpoint {
                    agent_id: 1,
                    address: "127.0.0.1:50052".into(),
                },
                AgentEndpoint {
                    agent_id: 2,
                    address: "127.0.0.1:50053".into(),
                },
            ],
            config: Some(GameConfig { tick_rate_ms: 50 }),
        })
        .await?;
    println!("StartGame OK");

    let mut stream = client.watch_game(WatchGameRequest {}).await?.into_inner();
    let mut seen = 0;
    while let Some(frame) = stream.message().await? {
        if frame.is_snapshot {
            let s = spectpb::SpectatorSnapshot::decode(frame.payload.as_slice())?;
            println!(
                "SNAPSHOT tick={} arena={:?} players={}",
                s.tick,
                s.arena.map(|a| (a.width, a.height)),
                s.players.len()
            );
        } else {
            let d = spectpb::SpectatorDelta::decode(frame.payload.as_slice())?;
            let new_blobs: usize = d.players.iter().map(|p| p.new_body.len()).sum();
            let alive = d.players.iter().filter(|p| p.alive).count();
            println!(
                "DELTA    tick={} players={} alive={} new_blobs={}",
                d.tick,
                d.players.len(),
                alive,
                new_blobs
            );
        }
        seen += 1;
        if seen >= 20 {
            break;
        }
    }
    println!("received {seen} frames");
    Ok(())
}
