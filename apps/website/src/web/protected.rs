use crate::agents::{AgentManager, CreateAgentRequest};
use crate::users::AuthSession;
use crate::web::layouts::pages;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Extension, Form, Router,
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

#[derive(Debug, serde::Deserialize)]
struct CreateAgentForm {
    name: String,
    source_code_url: String,
    dockerfile_path: Option<String>,
    context_sub_path: Option<String>,
}

mod post {
    use super::*;

    pub async fn new_agent(
        auth_session: AuthSession,
        agent_manager: Extension<Arc<AgentManager>>,
        Form(form): Form<CreateAgentForm>,
    ) -> impl IntoResponse {
        let user = if let Some(user) = auth_session.user {
            user
        } else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        // TODO: Validate input
        tracing::info!("Got create agent request: {:?}", form);

        let req = CreateAgentRequest {
            name: user.username + "/" + form.name.as_str(),
            git_repo: form.source_code_url,
            dockerfile_path: form.dockerfile_path,
            context_sub_path: form.context_sub_path,
        };

        if let Err(err) = agent_manager.create_agent(req).await {
            eprintln!("Failed to create agent: {:?}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Redirect::to("/agents").into_response()
    }
}
