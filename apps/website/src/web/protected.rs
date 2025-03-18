use crate::agents::{Agent, AgentManager};
use crate::users::AuthSession;
use crate::web::layouts::pages;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};

pub fn router() -> Router<AgentManager> {
    Router::new()
        .route("/agents", get(self::get::agents))
        .route("/agents/new", post(self::post::new_agent))
        .route("/settings", get(self::get::settings))
}

mod get {
    use super::*;

    pub async fn agents(
        auth_session: AuthSession,
        agent_manager: State<AgentManager>,
    ) -> impl IntoResponse {
        let user_id = match &auth_session.user {
            Some(user) => user.id,
            None => return StatusCode::UNAUTHORIZED.into_response(),
        };
        let agents = match agent_manager.get_agents_for_user(user_id).await {
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
        State(mut agent_manager): State<AgentManager>,
        Form(form): Form<CreateAgentForm>,
    ) -> impl IntoResponse {
        let user = if let Some(user) = auth_session.user {
            user
        } else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        tracing::info!("Got create agent request: {:?}", form);

        if form.name.is_empty() || form.source_code_url.is_empty() {
            return StatusCode::BAD_REQUEST.into_response();
        } else if form.name.len() > 20 || form.source_code_url.len() > 100 {
            return StatusCode::BAD_REQUEST.into_response();
        }

        // Treat empty strings as None
        let dockerfile_path = form.dockerfile_path.filter(|s| !s.is_empty());
        let context_sub_path = form.context_sub_path.filter(|s| !s.is_empty());

        let agent_name = form.name;
        let source_code_url = form.source_code_url;

        if let Err(err) = agent_manager
            .create_agent(
                agent_name,
                user.id,
                source_code_url,
                dockerfile_path,
                context_sub_path,
            )
            .await
        {
            eprintln!("Failed to create agent: {:?}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Redirect::to("/agents").into_response()
    }
}
