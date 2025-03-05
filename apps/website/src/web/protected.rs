use crate::agents::{AgentManager, CreateAgentRequest};
use crate::users::AuthSession;
use crate::web::layouts::pages;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Extension, Router,
};
use std::sync::Arc;

pub fn router() -> Router<()> {
    Router::new()
        .route("/agents", get(self::get::agents))
        .route("/agents/new", post(self::post::new_agent))
        .route("/settings", get(self::get::settings))
}

mod get {
    use super::*;

    pub async fn agents(
        auth_session: AuthSession,
        agent_manager: Extension<Arc<AgentManager>>,
    ) -> impl IntoResponse {
        let agents = match agent_manager.get_agents().await {
            Ok(agents) => agents,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        pages::agents(&auth_session, agents).into_response()
    }

    pub async fn settings(auth_session: AuthSession) -> impl IntoResponse {
        pages::settings(&auth_session)
    }
}

mod post {
    use super::*;

    pub async fn new_agent(
        auth_session: AuthSession,
        agent_manager: Extension<Arc<AgentManager>>,
    ) -> impl IntoResponse {
        // TODO: Validate input
        // TODO: put user_id in the agent name

        let req = CreateAgentRequest {
            name: "ageeeeent".to_string(),
            git_repo: "asdfsadf".to_string(),
            dockerfile_path: None,
            context_sub_path: None,
        };
        if let Err(err) = agent_manager.create_agent(req).await {
            eprintln!("Failed to create agent: {:?}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Redirect::to("/agents").into_response()
    }
}
