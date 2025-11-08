use crate::agents::manager::AgentManager;
use crate::users::AuthSession;
use crate::web::layouts::pages;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Router};

pub fn router() -> Router<AgentManager> {
    Router::new().route("/", get(self::get::index))
}

mod get {
    use super::*;

    pub async fn index(
        auth_session: AuthSession,
        State(agent_manager): State<AgentManager>,
    ) -> impl IntoResponse {
        let agents = match agent_manager.get_agents().await {
            Ok(agents) => agents,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        pages::home(&auth_session, agents).into_response()
    }
}
