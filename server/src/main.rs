use gameserver::games;
use gameserver::server;

#[tokio::main]
async fn main() {
    const NUMBER_OF_PLAYERS: usize = 4;
    server::GameServer::<NUMBER_OF_PLAYERS, games::achtung::Achtung>::new(
        games::achtung::AchtungConfig {
            arena_width: 1000,
            arena_height: 1000,
            edge_wrapping: false,
        },
    )
    .host_game()
    .await;
}
