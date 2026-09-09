use crate::web::layout::pages;
use crate::{
    users::Backend,
    web::{auth, oauth, protected, public, spectator},
};
use achtung_api::ApiState;
use achtung_config::{ResolvedCoordinator, WebsiteConfig};
use achtung_core::agents::manager::AgentManager;
use achtung_core::api_tokens::ApiTokenManager;
use achtung_core::registry::{RegistryClient, RegistryTokenManager};
use achtung_core::users::UserManager;
use agent_infra::{
    DockerMachineProviderConfig, MachineProvider, MicrosandboxMachineProviderConfig, Reaper,
    ReaperConfig,
};
use axum::{handler::HandlerWithoutStateExt, http::StatusCode};
use axum_login::{
    AuthManagerLayerBuilder, login_required,
    tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite},
};
use coordinator::{CoordinatorConfig, GameCoordinator, ImageUrl};
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
}

impl App {
    /// Build from an already-parsed [`WebsiteConfig`]. No `env::var` reads here:
    /// `main` parses once via `WebsiteConfig::from_env()` and passes it in, so
    /// missing/invalid vars fail fast with one human-readable error.
    pub async fn new(config: &WebsiteConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let client_id = ClientId::new(config.github_client_id.clone());
        let client_secret = ClientSecret::new(config.github_client_secret.clone());

        // JWT audience; must equal the registry's REGISTRY_AUTH_TOKEN_SERVICE
        // (compose sets both to `registry:5001`).
        // How this server reaches the registry API. Defaults to the host's
        // published port so `cargo run` outside compose works; compose
        // overrides it to `http://registry:5001` for in-network DNS.
        // Host rendered into user-facing `docker login/tag/push` hints. Users
        // run Docker on their own machine, so this is the externally reachable
        // address (`localhost:5001`), never the in-network name.

        let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())?;
        let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())?;
        let client = BasicClient::new(client_id, Some(client_secret), auth_url, Some(token_url));

        let db = achtung_core::db::connect_and_migrate(&config.database_url).await?;

        let registry_auth_config = RegistryAuthConfig::new(
            config.registry_private_key_pem.clone(),
            config.registry_service.clone(),
        )
        .map_err(|e| format!("invalid registry auth config: {e}"))?;

        let user_manager = UserManager::new(db.clone());
        let agent_manager = AgentManager::new(db.clone());
        let api_token_manager = ApiTokenManager::new(db.clone());
        let registry_token_manager =
            RegistryTokenManager::new(db.clone(), registry_auth_config.clone());
        let registry_client = RegistryClient::new(config.registry_url.clone());

        let state = AppState {
            agent_manager: agent_manager.clone(),
            api_token_manager: api_token_manager.clone(),
            registry_token_manager: registry_token_manager.clone(),
            registry_client: registry_client.clone(),
            registry_public_host: config.registry_public_host.clone(),
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

    pub async fn serve(
        self,
        addr: std::net::SocketAddr,
        coordinator: Option<ResolvedCoordinator>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Spectator target, shared between the coordinator (writer) and the SSE
        // relay (reader). Created unconditionally so the browser endpoint exists
        // even when the coordinator is disabled (it just returns UNAVAILABLE
        // until a game is running).
        let spectator_registry: coordinator::SpectatorRegistry =
            Arc::new(tokio::sync::RwLock::new(None));

        if let Some(coordinator_config) = coordinator {
            match coordinator_config.provider {
                achtung_config::MachineProviderKind::Docker => {
                    let name_prefix = coordinator_config.agent_name_prefix.clone();
                    let config = docker_config_from_resolved(&coordinator_config)
                        .map_err(|e| format!("invalid docker coordinator config: {e}"))?;
                    let provider = Arc::new(
                        agent_infra::DockerMachineProvider::new(config).map_err(|e| {
                            format!("Failed to create docker machine provider: {e}")
                        })?,
                    );
                    self.spawn_coordinator(
                        provider.clone(),
                        &coordinator_config,
                        spectator_registry.clone(),
                    );
                    self.spawn_reaper(provider, &coordinator_config);
                    let _ = name_prefix;
                }
                achtung_config::MachineProviderKind::Microsandbox => {
                    // Resolve the runtime here rather than mid-match: without it
                    // every spawn fails, and the first symptom would be a game
                    // that never starts.
                    agent_infra::ensure_runtime_installed().await.map_err(|e| {
                        format!("microsandbox runtime unavailable (requires /dev/kvm): {e}")
                    })?;
                    let config = microsandbox_config_from_resolved(&coordinator_config);
                    let provider = Arc::new(agent_infra::MicrosandboxMachineProvider::new(config));
                    self.spawn_coordinator(
                        provider.clone(),
                        &coordinator_config,
                        spectator_registry.clone(),
                    );
                    self.spawn_reaper(provider, &coordinator_config);
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

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app.into_make_service()).await?;

        Ok(())
    }

    fn spawn_coordinator<P: MachineProvider + 'static>(
        &self,
        provider: Arc<P>,
        resolved: &ResolvedCoordinator,
        spectator_registry: coordinator::SpectatorRegistry,
    ) {
        let game_host_image = ImageUrl::new(resolved.game_host_image.clone())
            .unwrap_or_else(|_| ImageUrl::from(resolved.game_host_image.clone()));

        let config = CoordinatorConfig {
            game_host_image,
            agents_per_game: resolved.agents_per_game,
            tick_rate_ms: resolved.tick_rate_ms,
            game_interval: std::time::Duration::from_secs(resolved.game_interval_secs),
            poll_interval: std::time::Duration::from_secs(1),
            game_host_grpc_port: resolved.game_host_grpc_port,
            agent_grpc_port: resolved.agent_grpc_port,
            // Generous enough to cover a microVM boot plus a cold image pull;
            // the coordinator retries and proceeds as soon as the host answers,
            // so a high ceiling costs nothing on a fast backend.
            game_host_connect_timeout: std::time::Duration::from_secs(
                resolved.game_host_connect_timeout_secs,
            ),
        };

        let coordinator = GameCoordinator::new(
            config,
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
        provider: Arc<P>,
        resolved: &ResolvedCoordinator,
    ) {
        let reaper_config = ReaperConfig {
            interval: std::time::Duration::from_secs(resolved.reaper_interval_secs),
            max_age: std::time::Duration::from_secs(resolved.reaper_max_age_secs),
            prefix: resolved.reaper_prefix.clone(),
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

fn docker_config_from_resolved(
    resolved: &ResolvedCoordinator,
) -> Result<DockerMachineProviderConfig, String> {
    // Validated at config parse time (`DOCKER_NETWORK` required for docker),
    // but return an error instead of panicking so `serve` stays panic-free.
    let network = resolved
        .docker_network
        .clone()
        .ok_or_else(|| "DOCKER_NETWORK is required when MACHINE_PROVIDER=docker".to_string())?;
    Ok(DockerMachineProviderConfig {
        network,
        registry_pull_host: resolved.registry_pull_host.clone(),
        name_prefix: resolved.agent_name_prefix.clone(),
    })
}

fn microsandbox_config_from_resolved(
    resolved: &ResolvedCoordinator,
) -> MicrosandboxMachineProviderConfig {
    // Whether a guest can reach a loopback-bound published port is undocumented.
    // If the game host cannot reach agents, widen MSB_HOST_BIND to 0.0.0.0 rather
    // than patching the provider.
    MicrosandboxMachineProviderConfig {
        cpus: resolved.machine_cpus,
        memory_mib: resolved.machine_memory_mib,
        host_port_base: resolved.msb_host_port_base,
        host_bind: resolved.msb_host_bind,
        registry_pull_host: resolved.registry_pull_host.clone(),
        registry_insecure: resolved.msb_registry_insecure,
        // Host-enforced backstop for a match that never reports completion; the
        // reaper is the slower second line of defence.
        max_duration_secs: resolved.msb_max_duration_secs,
    }
}
