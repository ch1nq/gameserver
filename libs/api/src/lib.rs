mod agents;
mod auth;
mod error;
mod registry;
mod tokens;

use achtung_core::agents::manager::AgentManager;
use achtung_core::api_tokens::ApiTokenManager;
use achtung_core::registry::{RegistryClient, RegistryTokenManager};
use achtung_core::users::UserManager;
use axum::Router;

#[derive(Clone)]
pub struct ApiState {
    pub user_manager: UserManager,
    pub agent_manager: AgentManager,
    pub api_token_manager: ApiTokenManager,
    pub token_manager: RegistryTokenManager,
    pub registry_client: RegistryClient,
}

/// Create the API router. Mount this under `/api/v1` in the host application.
pub fn router() -> Router<ApiState> {
    use api_types::routes;
    Router::new()
        .nest(routes::AGENTS_PREFIX, agents::router())
        .nest(routes::TOKENS_PREFIX, tokens::router())
        .nest(routes::REGISTRY_PREFIX, registry::router())
}
