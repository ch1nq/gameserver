use crate::ApiState;
use crate::error::ApiError;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use common::UserId;

/// Extractor that authenticates a request via Basic auth against API tokens.
///
/// Expects `Authorization: Basic base64("user-{id}:{token}")`.
pub struct ApiAuth(pub UserId);

impl FromRequestParts<ApiState> for ApiAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;

        let encoded = header
            .strip_prefix("Basic ")
            .ok_or(ApiError::Unauthorized)?;

        let decoded = BASE64.decode(encoded).map_err(|_| ApiError::Unauthorized)?;

        let decoded_str = String::from_utf8(decoded).map_err(|_| ApiError::Unauthorized)?;

        let (username, token) = decoded_str.split_once(':').ok_or(ApiError::Unauthorized)?;

        let user_id: UserId = username
            .strip_prefix("user-")
            .and_then(|id| id.parse().ok())
            .ok_or(ApiError::Unauthorized)?;

        state
            .api_token_manager
            .validate_token(&user_id, token)
            .await
            .map_err(|_| ApiError::Unauthorized)?;

        Ok(ApiAuth(user_id))
    }
}
