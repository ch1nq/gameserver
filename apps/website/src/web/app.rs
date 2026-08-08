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
use agent_infra::{
    DockerMachineProviderConfig, FirecrackerMachineProviderConfig, FlyMachineProviderConfig,
    FlyMachineProviderHost, MachineProvider, Reaper, ReaperConfig,
};
use axum::{handler::HandlerWithoutStateExt, http::StatusCode};
use axum_login::{
    AuthManagerLayerBuilder, login_required,
    tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite},
};
use coordinator::{CoordinatorConfig, GameCoordinator, ImageUrl};
use ipnet::Ipv4Net;
use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl, basic::BasicClient};
use registry_auth::RegistryAuthConfig;
use sqlx::PgPool;
use std::env;
use std::sync::Arc;
use time::Duration;
use tower::ServiceExt;
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
        // Spectator target, shared between the coordinator (writer) and the
        // gRPC-Web relay (reader). Created unconditionally so the browser
        // endpoint exists even when the coordinator is disabled (it just
        // returns UNAVAILABLE until a game is running).
        let spectator_registry: coordinator::SpectatorRegistry =
            Arc::new(tokio::sync::RwLock::new(None));

        if env::var("ENABLE_COORDINATOR").is_ok() {
            match env::var("MACHINE_PROVIDER").as_deref() {
                Ok("firecracker") => {
                    let config = firecracker_config_from_env();
                    let provider = Arc::new(
                        agent_infra::FirecrackerMachineProvider::new(config)
                            .await
                            .expect("Failed to connect to containerd"),
                    );
                    // Firecracker containers are ids like "achtung-<id>-slot-N".
                    self.spawn_coordinator_and_reaper(
                        provider,
                        "achtung-",
                        spectator_registry.clone(),
                    );
                }
                Ok("docker") => {
                    let config = docker_config_from_env();
                    let provider = Arc::new(
                        agent_infra::DockerMachineProvider::new(config)
                            .expect("Failed to connect to the Docker daemon"),
                    );
                    // Docker containers are named "achtung-<id>-slot-N".
                    self.spawn_coordinator_and_reaper(
                        provider,
                        "achtung-",
                        spectator_registry.clone(),
                    );
                }
                _ => {
                    let config = fly_config_from_env();
                    let provider = Arc::new(agent_infra::FlyMachineProvider::new(config));
                    // Fly apps are named "achtung-match-<id>-app".
                    self.spawn_coordinator_and_reaper(
                        provider,
                        "achtung-match-",
                        spectator_registry.clone(),
                    );
                }
            }
        }

        // Browser-facing spectator stream (gRPC-Web over the existing HTTP/1.1
        // server). Convert tonic's response body into an axum body so the
        // tonic service can be mounted as a plain route.
        let spectator_grpc =
            tonic_web::enable(coordinator::spectator_service(spectator_registry.clone()))
                .map_request(|req: axum::extract::Request| req.map(tonic::body::boxed))
                .map_response(|res: axum::http::Response<_>| res.map(axum::body::Body::new));

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
            .route_service("/spectator.Spectator/Watch", spectator_grpc)
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
        spectator_registry: coordinator::SpectatorRegistry,
    ) {
        self.spawn_coordinator(provider.clone(), spectator_registry);
        self.spawn_reaper(provider, reaper_prefix_default);
    }

    fn spawn_coordinator<P: MachineProvider + 'static>(
        &self,
        provider: Arc<P>,
        spectator_registry: coordinator::SpectatorRegistry,
    ) {
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
            spectator_registry,
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

fn fly_config_from_env() -> FlyMachineProviderConfig {
    FlyMachineProviderConfig {
        fly_token: env::var("FLY_TOKEN").expect("FLY_TOKEN required when MACHINE_PROVIDER=fly"),
        fly_org: env::var("FLY_ORG").expect("FLY_ORG required when MACHINE_PROVIDER=fly"),
        fly_host: match env::var("FLY_HOST").as_deref() {
            Ok("public") => FlyMachineProviderHost::Public,
            Ok("internal") | Err(_) => FlyMachineProviderHost::Internal,
            Ok(v) => panic!("Unknown FLY_HOST value: {v}"),
        },
        registry_url: env::var("REGISTRY_URL")
            .unwrap_or_else(|_| "https://achtung-registry.fly.dev".to_string()),
    }
}

fn docker_config_from_env() -> DockerMachineProviderConfig {
    DockerMachineProviderConfig {
        network: env::var("DOCKER_NETWORK")
            .expect("DOCKER_NETWORK required when MACHINE_PROVIDER=docker"),
        registry_pull_host: env::var("DOCKER_REGISTRY_PULL_HOST")
            .unwrap_or_else(|_| "localhost:5001".to_string()),
    }
}

fn firecracker_config_from_env() -> FirecrackerMachineProviderConfig {
    let defaults = FirecrackerMachineProviderConfig::default();
    FirecrackerMachineProviderConfig {
        containerd_socket: env::var("CONTAINERD_SOCKET").unwrap_or(defaults.containerd_socket),
        containerd_namespace: env::var("CONTAINERD_NAMESPACE")
            .unwrap_or(defaults.containerd_namespace),
        runtime: env::var("FIRECRACKER_RUNTIME").unwrap_or(defaults.runtime),
        kernel_path: env::var("FIRECRACKER_KERNEL")
            .expect("FIRECRACKER_KERNEL required when MACHINE_PROVIDER=firecracker"),
        vcpu_count: env::var("FIRECRACKER_VCPU_COUNT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(defaults.vcpu_count),
        mem_size_mib: env::var("FIRECRACKER_MEM_SIZE_MIB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(defaults.mem_size_mib),
        registry_url: env::var("REGISTRY_URL").unwrap_or(defaults.registry_url),
        subnet_pool: env::var("FIRECRACKER_SUBNET_POOL")
            .ok()
            .and_then(|s| s.parse::<Ipv4Net>().ok())
            .unwrap_or(defaults.subnet_pool),
    }
}
