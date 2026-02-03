use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Unauthorized")]
    Unauthorized,

    #[error("Not found")]
    _NotFound,

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            ApiError::_NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, self.to_string()),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = axum::Json(json!({ "error": message }));
        (status, body).into_response()
    }
}
