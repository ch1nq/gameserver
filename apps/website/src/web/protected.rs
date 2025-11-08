use crate::agents::agent::{AgentName, ImageUrl};
use crate::agents::manager::AgentManager;
use crate::users::AuthSession;
use crate::web::layouts::pages;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use std::str::FromStr;

pub fn router() -> Router<AgentManager> {
    Router::new()
        .route("/agents", get(self::get::agents))
        .route("/agents/new", post(self::post::new_agent))
        .route("/agents/{id}/activate", post(self::post::activate_agent))
        .route(
            "/agents/{id}/deactivate",
            post(self::post::deactivate_agent),
        )
        .route("/agents/{id}/delete", post(self::post::delete_agent))
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
    image_url: String,
}

mod post {
    use super::*;

    pub async fn new_agent(
        auth_session: AuthSession,
        State(agent_manager): State<AgentManager>,
        Form(form): Form<CreateAgentForm>,
    ) -> impl IntoResponse {
        let user = if let Some(user) = auth_session.user {
            user
        } else {
            return StatusCode::UNAUTHORIZED.into_response();
        };

        let name = match AgentName::from_str(&form.name) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("Invalid agent name: {}", e);
                return StatusCode::BAD_REQUEST.into_response();
            }
        };

        let image_url = match ImageUrl::new(form.image_url) {
            Ok(url) => url,
            Err(e) => {
                tracing::warn!("Invalid image URL: {}", e);
                return StatusCode::BAD_REQUEST.into_response();
            }
        };

        match agent_manager.create_agent(name, user.id, image_url).await {
            Ok(_) => Redirect::to("/agents").into_response(),
            Err(e) => {
                tracing::error!("Failed to create agent: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }

    pub async fn activate_agent(
        auth_session: AuthSession,
        State(agent_manager): State<AgentManager>,
        Path(agent_id): Path<i64>,
    ) -> impl IntoResponse {
        let user = if let Some(user) = auth_session.user {
            user
        } else {
            return StatusCode::UNAUTHORIZED.into_response();
        };

        match agent_manager.activate_agent(agent_id, user.id).await {
            Ok(_) => Redirect::to("/agents").into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }

    pub async fn deactivate_agent(
        auth_session: AuthSession,
        State(agent_manager): State<AgentManager>,
        Path(agent_id): Path<i64>,
    ) -> impl IntoResponse {
        let user = if let Some(user) = auth_session.user {
            user
        } else {
            return StatusCode::UNAUTHORIZED.into_response();
        };

        match agent_manager.deactivate_agent(agent_id, user.id).await {
            Ok(_) => Redirect::to("/agents").into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }

    pub async fn delete_agent(
        auth_session: AuthSession,
        State(agent_manager): State<AgentManager>,
        Path(agent_id): Path<i64>,
    ) -> impl IntoResponse {
        let user = if let Some(user) = auth_session.user {
            user
        } else {
            return StatusCode::UNAUTHORIZED.into_response();
        };

        match agent_manager.delete_agent(agent_id, user.id).await {
            Ok(_) => Redirect::to("/agents").into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}
