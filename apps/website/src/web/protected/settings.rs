use crate::registry::TokenName;
use crate::users::AuthSession;
use crate::web::app::AppState;
use crate::web::layout::pages;
use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use std::str::FromStr;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(settings))
        .route("/tokens/new", post(create_token))
        .route("/tokens/{id}/revoke", post(revoke_token))
}

async fn settings(auth_session: AuthSession, State(state): State<AppState>) -> impl IntoResponse {
    let user_id = match &auth_session.user {
        Some(user) => user.id,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let tokens = match state.token_manager.list_tokens(&user_id).await {
        Ok(tokens) => tokens,
        Err(e) => {
            tracing::error!("Failed to list tokens: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    pages::settings(&auth_session, tokens).into_response()
}

#[derive(Debug, serde::Deserialize)]
struct CreateTokenForm {
    name: String,
}

async fn create_token(
    auth_session: AuthSession,
    State(state): State<AppState>,
    Form(form): Form<CreateTokenForm>,
) -> impl IntoResponse {
    let user = if let Some(user) = auth_session.user {
        user
    } else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let token_name = match TokenName::from_str(&form.name) {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("Invalid token name: {}", e);
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    match state
        .token_manager
        .create_token(&user.id, &token_name)
        .await
    {
        Ok((token_id, plaintext_token)) => {
            // Return HTML response with modal showing the token
            pages::token_created(token_id, user.id, &plaintext_token).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to create token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn revoke_token(
    auth_session: AuthSession,
    State(state): State<AppState>,
    Path(token_id): Path<i64>,
) -> impl IntoResponse {
    let user = if let Some(user) = auth_session.user {
        user
    } else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match state.token_manager.revoke_token(&user.id, token_id).await {
        Ok(_) => Redirect::to("/settings").into_response(),
        Err(e) => {
            tracing::error!("Failed to revoke token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
