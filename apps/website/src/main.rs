use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use website::web::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(
            |_| {
                "website=debug,achtung-core=debug,coordinator=debug,axum_login=debug,tower_sessions=debug,sqlx=warn,tower_http=debug,registry-auth=debug"
                    .into()
            },
        )))
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    // Fetch address and port from environment variables.
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let addr: std::net::SocketAddr = format!("{}:{}", host, port).parse().unwrap();

    App::new().await?.serve(addr).await
}
