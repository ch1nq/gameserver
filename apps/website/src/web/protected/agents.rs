use crate::agents::agent::{AgentName, ImageUrl};
use crate::tournament_mananger;
use crate::users::AuthSession;
use crate::web::app::AppState;
use crate::web::layout::pages::{self, error_page};
use achtung_ui::error::Error;
use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use maud::Render;
use std::str::FromStr;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(agents))
        .route("/new", get(new_agent_page))
        .route("/new", post(new_agent))
        .route("/{id}/activate", post(activate_agent))
        .route("/{id}/deactivate", post(deactivate_agent))
        .route("/{id}/delete", post(delete_agent))
}

async fn agents(auth_session: AuthSession, State(state): State<AppState>) -> impl IntoResponse {
    let user_id = match &auth_session.user {
        Some(user) => user.id,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let mut errors = vec![];
    let agents = match state.agent_manager.get_agents_for_user(user_id).await {
        Ok(agents) => agents,
        Err(e) => {
            tracing::warn!("Failed to fetch agents for user: {}", e);
            errors.push(Error::internal_error("Failed to fetch agents for user"));
            vec![]
        }
    };
    pages::agents(&auth_session, agents)
        .with_errors(errors)
        .render()
        .into_response()
}

#[derive(Debug, serde::Deserialize)]
struct CreateAgentForm {
    name: String,
    image: String,
}

async fn new_agent_page(
    auth_session: AuthSession,
    State(mut state): State<AppState>,
) -> impl IntoResponse {
    let user = if let Some(user) = &auth_session.user {
        user
    } else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    // Get system token for registry authentication
    let system_token = match state.token_manager.get_system_token().await {
        Ok(token) => token,
        Err(e) => {
            tracing::error!("Failed to get system token: {}", e);
            return error_page(
                Error::internal_error("Failed to get system token"),
                &auth_session,
            )
            .render()
            .into_response();
        }
    };

    let credentials = tournament_mananger::RegistryCredentials {
        token: system_token.value.into(),
    };
    let request = tournament_mananger::ListImagesRequest {
        registry_credentials: Some(credentials),
        user_id: Some(tournament_mananger::UserId { id: user.id }),
    };
    match state.tournament_manager.list_images(request).await {
        Err(e) => {
            tracing::error!("Error getting list of user images: {}", e);
            return error_page(
                Error::internal_error("Error getting list of user images"),
                &auth_session,
            )
            .render()
            .into_response();
        }
        Ok(response) => {
            let user_images = response.into_inner().images;
            pages::new_agent_page(user_images, &auth_session)
                .render()
                .into_response()
        }
    }
}

async fn new_agent(
    auth_session: AuthSession,
    State(mut state): State<AppState>,
    Form(form): Form<CreateAgentForm>,
) -> impl IntoResponse {
    let user = if let Some(user) = &auth_session.user {
        user
    } else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let name = match AgentName::from_str(&form.name) {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("Invalid agent name: {}", e);
            return error_page(
                Error::validation_error(&format!("Invalid agent name: {}", e)),
                &auth_session,
            )
            .render()
            .into_response();
        }
    };

    let image_url = match ImageUrl::new(form.image) {
        Ok(url) => url,
        Err(e) => {
            tracing::warn!("Invalid image URL: {}", e);
            return error_page(
                Error::validation_error(&format!("Invalid image URL: {}", e)),
                &auth_session,
            )
            .render()
            .into_response();
        }
    };

    let system_token = match state.token_manager.get_system_token().await {
        Ok(token) => token,
        Err(e) => {
            tracing::error!("Failed to get system token: {}", e);
            return error_page(
                Error::internal_error("Failed to get system token"),
                &auth_session,
            )
            .render()
            .into_response();
        }
    };
    let request = tournament_mananger::CreateAgentRequest {
        name: name.clone().into(),
        registry_credentials: Some(tournament_mananger::RegistryCredentials {
            token: system_token.value.into(),
        }),
        image: Some(tournament_mananger::AgentImage {
            image_url: image_url.to_string(),
        }),
        owner: Some(tournament_mananger::UserId { id: user.id }),
    };

    if let Err(status) = state.tournament_manager.create_agent(request).await {
        tracing::error!("Failed to craete agent: {}", status);
        return error_page(
            Error::internal_error("Failed to craete agent"),
            &auth_session,
        )
        .render()
        .into_response();
    };

    match state
        .agent_manager
        .create_agent(name, user.id, image_url)
        .await
    {
        Ok(_) => Redirect::to("/agents").into_response(),
        Err(e) => {
            tracing::error!("Failed to create agent in db: {}", e);
            error_page(
                Error::internal_error("Failed to create agent in db"),
                &auth_session,
            )
            .render()
            .into_response()
        }
    }
}

async fn activate_agent(
    auth_session: AuthSession,
    State(state): State<AppState>,
    Path(agent_id): Path<i64>,
) -> impl IntoResponse {
    let user = if let Some(user) = auth_session.user {
        user
    } else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match state.agent_manager.activate_agent(agent_id, user.id).await {
        Ok(_) => Redirect::to("/agents").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn deactivate_agent(
    auth_session: AuthSession,
    State(state): State<AppState>,
    Path(agent_id): Path<i64>,
) -> impl IntoResponse {
    let user = if let Some(user) = auth_session.user {
        user
    } else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match state
        .agent_manager
        .deactivate_agent(agent_id, user.id)
        .await
    {
        Ok(_) => Redirect::to("/agents").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn delete_agent(
    auth_session: AuthSession,
    State(state): State<AppState>,
    Path(agent_id): Path<i64>,
) -> impl IntoResponse {
    let user = if let Some(user) = auth_session.user {
        user
    } else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match state.agent_manager.delete_agent(agent_id, user.id).await {
        Ok(_) => Redirect::to("/agents").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
