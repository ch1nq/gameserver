use gameserver::games;
use gameserver::server;

#[tokio::main]
async fn main() {
    const NUMBER_OF_PLAYERS: usize = 8;
    server::GameServer::<NUMBER_OF_PLAYERS, games::achtung::Achtung>::new()
        .host_game()
        .await;
}
