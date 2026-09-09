use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use achtung_config::WebsiteConfig;
use website::web::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Single entry-point parse: every env var is validated here with a
    // fail-fast, human-readable error. No `env::var(...).expect()` deeper in
    // `App::new` / `serve`.
    let config = match WebsiteConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(1);
        }
    };

    tracing_subscriber::registry()
        .with(EnvFilter::new(config.rust_log.clone()))
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    let addr = config.server.addr().map_err(|e| {
        eprintln!("configuration error: {e}");
        e
    })?;
    let coordinator = config.coordinator.clone();

    App::new(&config).await?.serve(addr, coordinator).await
}
