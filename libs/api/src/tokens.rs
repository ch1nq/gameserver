use crate::ApiState;
use crate::auth::ApiAuth;
use crate::error::ApiError;
use achtung_core::registry::TokenName;
use api_types::{CreateTokenRequest, CreateTokenResponse, routes};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use std::str::FromStr;

pub fn router() -> Router<ApiState> {
    Router::new()
        .route(routes::TOKENS, get(list_tokens))
        .route(routes::TOKENS, post(create_token))
        .route(routes::TOKEN, delete(revoke_token))
}

async fn list_tokens(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, ApiError> {
    let tokens = state
        .api_token_manager
        .list_tokens(&user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let tokens: Vec<api_types::ApiToken> = tokens.into_iter().map(Into::into).collect();
    Ok(Json(tokens))
}

async fn create_token(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let name = TokenName::from_str(&body.name).map_err(|e| ApiError::Validation(e.to_string()))?;

    let plaintext = state
        .api_token_manager
        .create_token(&user_id, &name)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let token_str: String = plaintext.into();
    Ok((
        StatusCode::CREATED,
        Json(CreateTokenResponse { token: token_str }),
    ))
}

async fn revoke_token(
    ApiAuth(user_id): ApiAuth,
    State(state): State<ApiState>,
    Path(token_id): Path<common::ApiTokenId>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .api_token_manager
        .revoke_token(&user_id, token_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
