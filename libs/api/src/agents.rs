use crate::ApiState;
use crate::auth::ApiAuth;
use crate::error::ApiError;
use achtung_core::agents::agent::AgentName;
use api_types::{CreateAgentRequest, routes};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use common::{AgentId, AgentImageUrl};
use std::str::FromStr;

pub fn router() -> Router<ApiState> {
    Router::new()
        .route(routes::AGENTS, get(list_agents))
        .route(routes::AGENTS, post(create_agent))
        .route(routes::AGENT_ACTIVATE, post(activate_agent))
        .route(routes::AGENT_DEACTIVATE, post(deactivate_agent))
        .route(routes::AGENT, delete(delete_agent))
}

async fn list_agents(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, ApiError> {
    let agents = state
        .agent_manager
        .get_agents_for_user(user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let agents: Vec<api_types::Agent> = agents.into_iter().map(Into::into).collect();
    Ok(Json(agents))
}

async fn create_agent(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
    Json(body): Json<CreateAgentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate agent name
    let name = AgentName::from_str(&body.name).map_err(ApiError::Validation)?;

    // Parse and validate agent image URL format
    let agent_image = AgentImageUrl::parse(user_id, &body.image)
        .map_err(|e| ApiError::Validation(e.to_string()))?;

    // Get system token for registry access
    let system_token = state
        .token_manager
        .get_system_token()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Validate image exists in user's registry namespace
    let image_exists = state
        .registry_client
        .image_exists(user_id, &agent_image, &system_token.value)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to validate image: {}", e)))?;

    if !image_exists {
        return Err(ApiError::Validation(format!(
            "Image '{}' not found in your registry namespace. Use 'achtung registry images' to see available images.",
            body.image
        )));
    }

    // Create agent - image is now validated
    let agent = state
        .agent_manager
        .create_agent(name, user_id, agent_image)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let agent: api_types::Agent = agent.into();
    Ok((StatusCode::CREATED, Json(agent)))
}

async fn activate_agent(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
    Path(agent_id): Path<AgentId>,
) -> Result<impl IntoResponse, ApiError> {
    let agent = state
        .agent_manager
        .activate_agent(agent_id, user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let agent: api_types::Agent = agent.into();
    Ok(Json(agent))
}

async fn deactivate_agent(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
    Path(agent_id): Path<AgentId>,
) -> Result<impl IntoResponse, ApiError> {
    let agent = state
        .agent_manager
        .deactivate_agent(agent_id, user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let agent: api_types::Agent = agent.into();
    Ok(Json(agent))
}

async fn delete_agent(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
    Path(agent_id): Path<AgentId>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .agent_manager
        .delete_agent(agent_id, user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
