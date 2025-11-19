use crate::users::AuthSession;
use crate::web::app::AppState;
use crate::web::pages;
use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(self::get::index))
}

mod get {
    use super::*;

    pub async fn index(
        auth_session: AuthSession,
        State(state): State<AppState>,
    ) -> impl IntoResponse {
        let agents = match state.agent_manager.get_agents().await {
            Ok(agents) => agents,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        pages::home(&auth_session, agents).into_response()
    }
}
