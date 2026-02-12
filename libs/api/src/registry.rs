use crate::ApiState;
use crate::auth::ApiAuth;
use crate::error::ApiError;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use common::AgentImageUrl;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ValidateImageQuery {
    image: String,
}

pub fn router() -> Router<ApiState> {
    Router::new()
        .route(api_types::routes::IMAGES, get(list_images))
        .route(api_types::routes::VALIDATE_IMAGE, get(validate_image))
}

async fn list_images(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, ApiError> {
    let system_token = state
        .token_manager
        .get_system_token()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let images = state
        .registry_client
        .list_user_images(user_id, &system_token.value)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(images))
}

async fn validate_image(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
    Query(query): Query<ValidateImageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Parse and validate image URL format
    let agent_image = AgentImageUrl::parse(user_id, &query.image)
        .map_err(|e| ApiError::Validation(e.to_string()))?;

    // Get system token for registry access
    let system_token = state
        .token_manager
        .get_system_token()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Check if image exists in registry
    let exists = state
        .registry_client
        .image_exists(user_id, &agent_image, &system_token.value)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to validate image: {}", e)))?;

    if !exists {
        return Err(ApiError::Validation(format!(
            "Image '{}' not found in your registry namespace",
            query.image
        )));
    }

    // Return the validated image
    Ok(Json(agent_image))
}
