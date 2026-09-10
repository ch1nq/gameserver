use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use website::config::Config;
use website::web::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(
            |_| {
                "website=debug,achtung-core=debug,coordinator=debug,achtung-api=debug,agent_infra=debug,axum_login=debug,tower_sessions=debug,sqlx=warn,tower_http=debug,registry-auth=debug"
                    .into()
            },
        )))
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    // Single fail-fast parse of all startup configuration. A missing var or an
    // invalid value is reported here as one human-readable line, rather than
    // panicking deep inside `serve`.
    let config = match Config::load() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Configuration error: {e}");
            std::process::exit(1);
        }
    };

    let addr = config.server.socket_addr()?;

    App::new(config).await?.serve(addr).await
}
