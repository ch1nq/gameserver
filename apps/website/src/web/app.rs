use crate::config::{Config, CoordinatorSettings, ProviderConfig};
use crate::web::layout::pages;
use crate::{
    users::Backend,
    web::{auth, oauth, protected, public, spectator},
};
use achtung_api::ApiState;
use achtung_core::agents::manager::AgentManager;
use achtung_core::api_tokens::ApiTokenManager;
use achtung_core::registry::{RegistryClient, RegistryTokenManager};
use achtung_core::users::UserManager;
use agent_infra::{MachineProvider, Reaper};
use axum::{handler::HandlerWithoutStateExt, http::StatusCode};
use axum_login::{
    AuthManagerLayerBuilder, login_required,
    tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite},
};
use coordinator::GameCoordinator;
use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl, basic::BasicClient};
use registry_auth::RegistryAuthConfig;
use sqlx::PgPool;
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
    /// Host users type into `docker login/tag/push` (reachable from their
    /// machine, e.g. `localhost:5001`). Deliberately separate from
    /// `REGISTRY_SERVICE` (the JWT `aud`, which must match the registry's
    /// `REGISTRY_AUTH_TOKEN_SERVICE`) and `REGISTRY_URL` (how this server
    /// reaches the registry API over the compose network).
    pub registry_public_host: String,
}

pub struct App {
    db: PgPool,
    client: BasicClient,
    state: AppState,
    api_state: ApiState,
    registry_auth_config: RegistryAuthConfig,
    /// Coordinator settings, present only when the coordinator is enabled.
    coordinator: Option<CoordinatorSettings>,
}

impl App {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let client_id = ClientId::new(config.github.client_id);
        let client_secret = ClientSecret::new(config.github.client_secret);

        let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())?;
        let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())?;
        let client = BasicClient::new(client_id, Some(client_secret), auth_url, Some(token_url));

        let db = achtung_core::db::connect_and_migrate(&config.database_url).await?;

        let registry_auth_config =
            RegistryAuthConfig::new(config.registry.private_key_pem, config.registry.service)
                .map_err(|e| format!("invalid REGISTRY_PRIVATE_KEY / registry auth config: {e}"))?;

        let user_manager = UserManager::new(db.clone());
        let agent_manager = AgentManager::new(db.clone());
        let api_token_manager = ApiTokenManager::new(db.clone());
        let registry_token_manager =
            RegistryTokenManager::new(db.clone(), registry_auth_config.clone());
        let registry_client = RegistryClient::new(config.registry.url);

        let state = AppState {
            agent_manager: agent_manager.clone(),
            api_token_manager: api_token_manager.clone(),
            registry_token_manager: registry_token_manager.clone(),
            registry_client: registry_client.clone(),
            registry_public_host: config.registry.public_host,
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
            coordinator: config.coordinator,
        })
    }

    pub async fn serve(self, addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        // Spectator target, shared between the coordinator (writer) and the SSE
        // relay (reader). Created unconditionally so the browser endpoint exists
        // even when the coordinator is disabled (it just returns UNAVAILABLE
        // until a game is running).
        let spectator_registry: coordinator::SpectatorRegistry =
            Arc::new(tokio::sync::RwLock::new(None));

        if let Some(coordinator) = self.coordinator.clone() {
            match coordinator.provider.clone() {
                ProviderConfig::Docker(config) => {
                    let provider = Arc::new(
                        agent_infra::DockerMachineProvider::new(config).map_err(|e| {
                            format!("Failed to create docker machine provider: {e}")
                        })?,
                    );
                    self.spawn_coordinator(
                        &coordinator,
                        provider.clone(),
                        spectator_registry.clone(),
                    );
                    self.spawn_reaper(&coordinator, provider);
                }
                ProviderConfig::Microsandbox(config) => {
                    // Resolve the runtime here rather than mid-match: without it
                    // every spawn fails, and the first symptom would be a game
                    // that never starts.
                    agent_infra::ensure_runtime_installed().await.map_err(|e| {
                        format!("microsandbox runtime unavailable (requires /dev/kvm): {e}")
                    })?;
                    let provider = Arc::new(agent_infra::MicrosandboxMachineProvider::new(config));
                    self.spawn_coordinator(
                        &coordinator,
                        provider.clone(),
                        spectator_registry.clone(),
                    );
                    self.spawn_reaper(&coordinator, provider);
                }
            }
        }

        // Browser-facing spectator stream, served as Server-Sent Events. Decodes
        // the current game host's WatchGame stream and re-emits JSON, so the
        // browser needs no protobuf/gRPC runtime.
        let spectator_router =
            spectator::router(spectator::SpectatorState::new(spectator_registry.clone()));

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
            .merge(spectator_router)
            .nest("/api/v1", api_router)
            .nest_service("/static", static_service)
            .fallback_service(fallback_service)
            .merge(services);

        println!("Serving on {addr}");

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app.into_make_service()).await?;

        Ok(())
    }

    fn spawn_coordinator<P: MachineProvider + 'static>(
        &self,
        settings: &CoordinatorSettings,
        provider: Arc<P>,
        spectator_registry: coordinator::SpectatorRegistry,
    ) {
        let coordinator = GameCoordinator::new(
            settings.coordinator_config(),
            provider,
            Box::new(self.state.agent_manager.clone()),
            Box::new(self.state.registry_token_manager.clone()),
            spectator_registry,
        );
        coordinator.spawn();

        tracing::info!("Game coordinator spawned");
    }

    fn spawn_reaper<P: MachineProvider + 'static>(
        &self,
        settings: &CoordinatorSettings,
        provider: Arc<P>,
    ) {
        let reaper_config = settings.reaper.clone();

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
