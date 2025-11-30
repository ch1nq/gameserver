use crate::users::AuthSession;
use crate::web::app::AppState;
use crate::web::layout::pages;
use achtung_ui::error::Error;
use axum::{Router, extract::State, response::IntoResponse, routing::get};
use maud::Render;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(self::get::index))
}

mod get {

    use super::*;

    pub async fn index(
        auth_session: AuthSession,
        State(state): State<AppState>,
    ) -> impl IntoResponse {
        let (agents, error) = match state.agent_manager.get_agents().await {
            Ok(agents) => (agents, None),
            Err(_) => (
                vec![],
                Some(Error::internal_error("Failed to fetch active agents")),
            ),
        };
        pages::home(&auth_session, agents)
            .with_errors(error.into_iter().collect())
            .render()
            .into_response()
    }
}
