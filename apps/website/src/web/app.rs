use crate::agents::manager::AgentManager;
use crate::registry::{RegistryClient, TokenManager};
use crate::web::layout::pages;
use crate::{
    users::Backend,
    web::{auth, oauth, protected, public},
};
use agent_infra::FlyMachineProviderConfig;
use axum::{handler::HandlerWithoutStateExt, http::StatusCode};
use axum_login::{
    AuthManagerLayerBuilder, login_required,
    tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite},
};
use coordinator::{CoordinatorConfig, GameCoordinator};
use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl, basic::BasicClient};
use registry_auth::RegistryAuthConfig;
use sqlx::PgPool;
use std::env;
use time::Duration;
use tower_http::services::ServeDir;
use tower_sessions_sqlx_store::PostgresStore;

#[derive(Clone)]
pub struct AppState {
    pub agent_manager: AgentManager,
    pub token_manager: TokenManager,
    pub registry_client: RegistryClient,
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
        let private_key_pem = env::var("REGISTRY_PRIVATE_KEY")
            .expect("REGISTRY_PRIVATE_KEY must be set for registry authentication (RSA private key in PEM format)");
        let registry_service =
            env::var("REGISTRY_SERVICE").unwrap_or_else(|_| "achtung-registry.fly.dev".to_string());
        let registry_url = env::var("REGISTRY_URL")
            .unwrap_or_else(|_| format!("https://{}", registry_service));

        let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())?;
        let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())?;
        let client = BasicClient::new(client_id, Some(client_secret), auth_url, Some(token_url));

        let db_connection_str = std::env::var("DATABASE_URL").expect("Database url not defined");
        let db = PgPool::connect(&db_connection_str).await?;
        sqlx::migrate!().run(&db).await?;

        let registry_auth_config =
            RegistryAuthConfig::new(private_key_pem, registry_service)
                .expect("Failed to create registry auth config");

        let agent_manager = AgentManager::new(db.clone());
        let token_manager = TokenManager::new(db.clone(), registry_auth_config.clone());
        let registry_client = RegistryClient::new(registry_url);

        let state = AppState {
            agent_manager,
            token_manager,
            registry_client,
        };

        Ok(Self {
            db,
            client,
            state,
            registry_auth_config,
        })
    }

    pub async fn serve(self, addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        // Optionally spawn the game coordinator
        if env::var("ENABLE_COORDINATOR").is_ok() {
            self.spawn_coordinator();
        }

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
        let registry_router = registry_auth::router(
            self.state.token_manager.clone(),
            self.registry_auth_config,
        );

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

    fn spawn_coordinator(&self) {
        let fly_token = env::var("FLY_TOKEN").expect("FLY_TOKEN required for coordinator");
        let fly_org = env::var("FLY_ORG").expect("FLY_ORG required for coordinator");
        let registry_url = env::var("REGISTRY_URL")
            .unwrap_or_else(|_| "https://achtung-registry.fly.dev".to_string());
        let game_host_image = env::var("GAME_HOST_IMAGE")
            .unwrap_or_else(|_| "achtung-game-host:latest".to_string());

        let config = CoordinatorConfig {
            machine_provider: FlyMachineProviderConfig {
                fly_token,
                fly_org,
                fly_host: agent_infra::FlyMachineProviderHost::Internal,
                registry_url,
            },
            game_host_image,
            agents_per_game: env::var("AGENTS_PER_GAME")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4),
            tick_rate_ms: env::var("GAME_TICK_RATE_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
            arena_width: 1000,
            arena_height: 1000,
            game_interval: std::time::Duration::from_secs(
                env::var("GAME_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
            ),
            poll_interval: std::time::Duration::from_secs(1),
            game_host_grpc_port: 50051,
            agent_grpc_port: 50052,
        };

        let coordinator = GameCoordinator::new(config, self.state.agent_manager.clone());
        coordinator.spawn();

        tracing::info!("Game coordinator spawned");
    }
}
