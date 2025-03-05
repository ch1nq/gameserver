use crate::agents::AgentManager;
use crate::{
    users::Backend,
    web::{auth, layouts::pages, oauth, protected, public},
};
use axum::{handler::HandlerWithoutStateExt, http::StatusCode, Extension};
use axum_login::{
    login_required,
    tower_sessions::{cookie::SameSite, Expiry, SessionManagerLayer},
    AuthManagerLayerBuilder,
};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, TokenUrl};
use sqlx::PgPool;
use std::env;
use std::sync::Arc;
use time::Duration;
use tower_http::services::ServeDir;
use tower_sessions_sqlx_store::PostgresStore;

pub struct App {
    db: PgPool,
    client: BasicClient,
    agent_manager: Arc<AgentManager>,
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

        let db_connection_str = std::env::var("DATABASE_URL").expect("Database url not defined");
        let db = PgPool::connect(&db_connection_str).await?;
        sqlx::migrate!().run(&db).await?;

        let reqwest_client = reqwest::Client::new();
        let agent_manager_url =
            env::var("AGENT_MANAGER_URL").expect("AGENT_MANAGER_URL not defined");
        let agent_manager = Arc::new(AgentManager::new(reqwest_client, agent_manager_url));

        Ok(Self {
            db,
            client,
            agent_manager,
        })
    }

    pub async fn serve(self, addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        // Static files service
        let static_service = ServeDir::new("static");

        // Fallback service
        let fallback_service = (StatusCode::NOT_FOUND, pages::not_found()).into_service();

        // Session layer
        let session_store = PostgresStore::new(self.db.clone());
        session_store.migrate().await?;

        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_same_site(SameSite::Lax)
            .with_expiry(Expiry::OnInactivity(Duration::days(1)));

        // Auth service
        let backend = Backend::new(self.db, self.client);
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        // Agent manager service
        let agent_manager_layer = Extension(self.agent_manager);

        let services = protected::router()
            .route_layer(login_required!(Backend, login_url = "/login"))
            .merge(auth::router())
            .merge(oauth::router())
            .merge(public::router())
            .layer(agent_manager_layer)
            .layer(auth_layer);

        let app = axum::Router::new()
            .nest_service("/static", static_service)
            .fallback_service(fallback_service)
            .merge(services);

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app.into_make_service()).await?;

        Ok(())
    }
}
