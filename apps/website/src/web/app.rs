use crate::agents::manager::AgentManager;
use crate::registry;
use crate::registry::TokenManager;
use crate::registry::auth::RegistryAuthConfig;
use crate::tournament_mananger::tournament_manager_client::TournamentManagerClient;
use crate::web::layout::pages;
use crate::{
    users::Backend,
    web::{auth, oauth, protected, public},
};
use axum::{handler::HandlerWithoutStateExt, http::StatusCode};
use axum_login::{
    AuthManagerLayerBuilder, login_required,
    tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite},
};
use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl, basic::BasicClient};
use sqlx::PgPool;
use std::env;
use time::Duration;
use tonic::transport::Channel;
use tower_http::services::ServeDir;
use tower_sessions_sqlx_store::PostgresStore;

#[derive(Clone)]
pub struct AppState {
    pub agent_manager: AgentManager,
    pub token_manager: TokenManager,
    pub tournament_manager: TournamentManagerClient<Channel>,
}

pub struct App {
    db: PgPool,
    client: BasicClient,
    state: AppState,
    registry_auth_config: RegistryAuthConfig,
}

impl App {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let client_id = env::var("GITHUB_CLIENT_ID")
            .map(ClientId::new)
            .expect("GITHUB_CLIENT_ID should be provided.");
        let client_secret = env::var("GITHUB_CLIENT_SECRET")
            .map(ClientSecret::new)
            .expect("GITHUB_CLIENT_SECRET should be provided");
        let tournament_manager_url =
            env::var("TOURNAMENT_MANAGER_URL").expect("TOURNAMENT_MANAGER_URL should be provided");
        let private_key_pem = env::var("REGISTRY_PRIVATE_KEY")
            .expect("REGISTRY_PRIVATE_KEY must be set for registry authentication (RSA private key in PEM format)");
        let registry_service =
            env::var("REGISTRY_SERVICE").unwrap_or_else(|_| "achtung-registry.fly.dev".to_string());

        let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())?;
        let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())?;
        let client = BasicClient::new(client_id, Some(client_secret), auth_url, Some(token_url));

        let db_connection_str = std::env::var("DATABASE_URL").expect("Database url not defined");
        let db = PgPool::connect(&db_connection_str).await?;
        sqlx::migrate!().run(&db).await?;

        let registry_auth_config =
            registry::auth::RegistryAuthConfig::new(private_key_pem, registry_service)
                .expect("Failed to create registry auth config");

        let agent_manager = AgentManager::new(db.clone());
        let token_manager = TokenManager::new(db.clone(), registry_auth_config.clone());
        let tournament_manager = TournamentManagerClient::connect(tournament_manager_url).await?;

        let state = AppState {
            agent_manager,
            token_manager,
            tournament_manager,
        };

        Ok(Self {
            db,
            client,
            state,
            registry_auth_config,
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
        let backend = Backend::new(self.db.clone(), self.client);
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        // Registry auth router
        let registry_router =
            registry::auth::router(self.state.token_manager.clone(), self.registry_auth_config);

        let services = protected::router()
            .route_layer(login_required!(Backend, login_url = "/login"))
            .merge(public::router())
            .with_state(self.state)
            .merge(auth::router())
            .merge(oauth::router())
            .nest("/registry", registry_router)
            .layer(auth_layer);

        let app = axum::Router::new()
            .nest_service("/static", static_service)
            .fallback_service(fallback_service)
            .merge(services);

        println!("Serving on {addr}");

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app.into_make_service()).await?;

        Ok(())
    }
}
