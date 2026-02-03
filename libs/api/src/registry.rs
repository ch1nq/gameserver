use crate::ApiState;
use crate::auth::ApiAuth;
use crate::error::ApiError;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

pub fn router() -> Router<ApiState> {
    Router::new().route(api_types::routes::IMAGES, get(list_images))
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
