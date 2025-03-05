use crate::agents::AgentManager;
use crate::users::AuthSession;
use crate::web::layouts::pages;
use axum::{http::StatusCode, response::IntoResponse, routing::get, Extension, Router};
use std::sync::Arc;

pub fn router() -> Router<()> {
    Router::new().route("/", get(self::get::index))
}

mod get {
    use super::*;

    pub async fn index(
        auth_session: AuthSession,
        agent_manager: Extension<Arc<AgentManager>>,
    ) -> impl IntoResponse {
        let agents = match agent_manager.get_agents().await {
            Ok(agents) => agents,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        pages::home(&auth_session, agents).into_response()
    }
}
