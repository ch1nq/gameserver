use crate::web::layout::pages;
use crate::{
    users::Backend,
    web::{auth, oauth, protected, public},
};
use achtung_api::ApiState;
use achtung_core::agents::manager::AgentManager;
use achtung_core::api_tokens::ApiTokenManager;
use achtung_core::registry::{RegistryClient, RegistryTokenManager};
use achtung_core::users::UserManager;
use agent_infra::{DockerMachineProviderConfig, MachineProvider, Reaper, ReaperConfig};
use axum::{handler::HandlerWithoutStateExt, http::StatusCode};
use axum_login::{
    AuthManagerLayerBuilder, login_required,
    tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite},
};
use coordinator::ImageUrl;
use coordinator::{CoordinatorConfig, GameCoordinator};
use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl, basic::BasicClient};
use registry_auth::RegistryAuthConfig;
use sqlx::PgPool;
use std::env;
use std::sync::Arc;
use time::Duration;
use tower_http::services::ServeDir;
use tower_sessions_sqlx_store::PostgresStore;

#[derive(Clone)]
pub struct AppState {
    pub agent_manager: AgentManager,
    pub api_token_manager: ApiTokenManager,
    pub registry_token_manager: RegistryTokenManager,
    pub registry_client: RegistryClient,
}

pub struct App {
    db: PgPool,
    client: BasicClient,
    state: AppState,
    api_state: ApiState,
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
        let registry_url =
            env::var("REGISTRY_URL").unwrap_or_else(|_| format!("https://{}", registry_service));

        let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())?;
        let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())?;
        let client = BasicClient::new(client_id, Some(client_secret), auth_url, Some(token_url));

        let db_connection_str = std::env::var("DATABASE_URL").expect("Database url not defined");
        let db = achtung_core::db::connect_and_migrate(&db_connection_str).await?;

        let registry_auth_config = RegistryAuthConfig::new(private_key_pem, registry_service)
            .expect("Failed to create registry auth config");

        let user_manager = UserManager::new(db.clone());
        let agent_manager = AgentManager::new(db.clone());
        let api_token_manager = ApiTokenManager::new(db.clone());
        let registry_token_manager =
            RegistryTokenManager::new(db.clone(), registry_auth_config.clone());
        let registry_client = RegistryClient::new(registry_url);

        let state = AppState {
            agent_manager: agent_manager.clone(),
            api_token_manager: api_token_manager.clone(),
            registry_token_manager: registry_token_manager.clone(),
            registry_client: registry_client.clone(),
        };

        let api_state = ApiState {
            user_manager,
            agent_manager,
            api_token_manager,
            token_manager: registry_token_manager,
            registry_client,
        };

        Ok(Self {
            db,
            client,
            state,
            api_state,
            registry_auth_config,
        })
    }

    pub async fn serve(self, addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        if env::var("ENABLE_COORDINATOR").is_ok() {
            let config = docker_config_from_env();
            let provider = Arc::new(
                agent_infra::DockerMachineProvider::new(config)
                    .expect("Failed to connect to the Docker daemon"),
            );
            // Docker containers are named "achtung-<id>-slot-N".
            self.spawn_coordinator_and_reaper(provider, "achtung-");
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
            self.state.registry_token_manager.clone(),
            self.registry_auth_config,
        );

        // API router (stateless Basic auth, no session layer)
        let api_router = achtung_api::router().with_state(self.api_state);

        let services = protected::router()
            .route_layer(login_required!(Backend, login_url = "/login"))
            .merge(public::router())
            .with_state(self.state)
            .merge(auth::router())
            .merge(oauth::router())
            .nest("/registry", registry_router)
            .layer(auth_layer);

        let app = axum::Router::new()
            .nest("/api/v1", api_router)
            .nest_service("/static", static_service)
            .fallback_service(fallback_service)
            .merge(services);

        println!("Serving on {addr}");

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app.into_make_service()).await?;

        Ok(())
    }

    /// Spawn coordinator and reaper sharing a single `Arc<P>` provider.
    ///
    /// `reaper_prefix_default` is the backend-specific default used to match this
    /// provider's resources when `REAPER_PREFIX` is not set in the environment.
    fn spawn_coordinator_and_reaper<P: MachineProvider + 'static>(
        &self,
        provider: Arc<P>,
        reaper_prefix_default: &str,
    ) {
        self.spawn_coordinator(provider.clone());
        self.spawn_reaper(provider, reaper_prefix_default);
    }

    fn spawn_coordinator<P: MachineProvider + 'static>(&self, provider: Arc<P>) {
        let game_host_image = env::var("GAME_HOST_IMAGE")
            .unwrap_or_else(|_| "ghcr.io/ch1nq/achtung-game-host:latest".to_string());
        let game_host_image =
            ImageUrl::new(game_host_image).expect("GAME_HOST_IMAGE must be a valid image URL");

        let config = CoordinatorConfig {
            game_host_image,
            agents_per_game: env::var("AGENTS_PER_GAME")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4),
            tick_rate_ms: env::var("GAME_TICK_RATE_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
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

        let coordinator = GameCoordinator::new(
            config,
            provider,
            Box::new(self.state.agent_manager.clone()),
            Box::new(self.state.registry_token_manager.clone()),
        );
        coordinator.spawn();

        tracing::info!("Game coordinator spawned");
    }

    fn spawn_reaper<P: MachineProvider + 'static>(
        &self,
        provider: Arc<P>,
        reaper_prefix_default: &str,
    ) {
        let reaper_config = ReaperConfig {
            interval: std::time::Duration::from_secs(
                env::var("REAPER_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(300),
            ),
            max_age: std::time::Duration::from_secs(
                env::var("REAPER_MAX_AGE_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(3600),
            ),
            prefix: env::var("REAPER_PREFIX").unwrap_or_else(|_| reaper_prefix_default.to_string()),
        };

        let interval = reaper_config.interval;
        let max_age = reaper_config.max_age;
        let prefix = reaper_config.prefix.clone();

        let reaper = Reaper::new(provider, reaper_config);
        reaper.spawn();

        tracing::info!(
            "Infrastructure reaper spawned: interval={:?}, max_age={:?}, prefix={}",
            interval,
            max_age,
            prefix
        );
    }
}

fn docker_config_from_env() -> DockerMachineProviderConfig {
    DockerMachineProviderConfig {
        network: env::var("DOCKER_NETWORK")
            .expect("DOCKER_NETWORK required when the coordinator is enabled"),
        registry_pull_host: env::var("DOCKER_REGISTRY_PULL_HOST")
            .unwrap_or_else(|_| "localhost:5001".to_string()),
    }
}
