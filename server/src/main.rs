use gameserver::games;
use gameserver::server;

#[tokio::main]
async fn main() {
    const NUMBER_OF_PLAYERS: usize = 8;
    server::GameServer::<NUMBER_OF_PLAYERS, games::achtung::Achtung>::new(
        Some(tokio::time::Duration::from_nanos(500_000)),
        // None,
        games::achtung::AchtungConfig {
            arena_width: 1000,
            arena_height: 1000,
            edge_wrapping: false,
        },
    )
    .host_game()
    .await;
}
