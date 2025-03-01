use std::env;

use crate::{
    users::Backend,
    web::{auth, frontpage, oauth, protected},
};
use axum_login::{
    login_required,
    tower_sessions::{cookie::SameSite, Expiry, MemoryStore, SessionManagerLayer},
    AuthManagerLayerBuilder,
};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, TokenUrl};
use sqlx::SqlitePool;
use time::Duration;
use tower_http::services::ServeDir;

pub struct App {
    db: SqlitePool,
    client: BasicClient,
}

impl App {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let client_id = env::var("GITHUB_CLIENT_ID")
            .map(ClientId::new)
            .expect("GITHUB_CLIENT_ID should be provided.");
        let client_secret = env::var("GITHUB_CLIENT_SECRET")
            .map(ClientSecret::new)
            .expect("GITHUB_CLIENT_SECRET should be provided");

        let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())?;
        let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())?;
        let client = BasicClient::new(client_id, Some(client_secret), auth_url, Some(token_url));

        let db = SqlitePool::connect(":memory:").await?;
        sqlx::migrate!().run(&db).await?;

        Ok(Self { db, client })
    }

    pub async fn serve(self, addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        // Session layer.
        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_same_site(SameSite::Lax)
            .with_expiry(Expiry::OnInactivity(Duration::days(1)));

        // Auth service.
        let backend = Backend::new(self.db, self.client);
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        // Static files service
        let static_service = ServeDir::new("static");

        let app = axum::Router::new()
            .merge(frontpage::router())
            .merge(
                protected::router()
                    .route_layer(login_required!(Backend, login_url = "/login"))
                    .merge(auth::router())
                    .merge(oauth::router())
                    .layer(auth_layer),
            )
            .nest_service("/static", static_service);

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app.into_make_service()).await?;

        Ok(())
    }
}
