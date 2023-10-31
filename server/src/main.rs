use clap::Parser;
use gameserver::games;
use gameserver::server;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "3030")]
    port: u16,

    #[arg(short, long)]
    tick_rate_ms: Option<u64>,

    #[arg(short, long)]
    game: Game,

    #[arg(short, long)]
    num_players: usize,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Game {
    Achtung,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let tick_rate = cli.tick_rate_ms.map(tokio::time::Duration::from_millis);
    let server = match cli.game {
        Game::Achtung => server::GameServer::<games::achtung::Achtung>::new(
            tick_rate,
            games::achtung::AchtungConfig {
                arena_width: 1000,
                arena_height: 1000,
                edge_wrapping: false,
            },
            cli.num_players,
        ),
    };
    server.host_game(cli.port).await;
}
