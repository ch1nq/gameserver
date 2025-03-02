//! Run with
//!
//! ```not_rust
//! cargo run -p example-oauth2
//! ```
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::web::App;

mod users;
mod web;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(
            |_| "axum_login=debug,tower_sessions=debug,sqlx=warn,tower_http=debug".into(),
        )))
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    // Fetch address and port from environment variables.
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port.parse().unwrap()));

    App::new().await?.serve(addr).await
}
