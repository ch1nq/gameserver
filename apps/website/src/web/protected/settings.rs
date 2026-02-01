use crate::users::AuthSession;
use crate::web::app::AppState;
use crate::web::layout::pages;
use achtung_core::registry::TokenName;
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
        .route("/", get(settings))
        .route("/tokens/new", post(create_token))
        .route("/tokens/{id}/revoke", post(revoke_token))
}

async fn settings(auth_session: AuthSession, State(state): State<AppState>) -> impl IntoResponse {
    let user = match &auth_session.user {
        Some(user) => user,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let mut errors = vec![];
    let tokens = match state.token_manager.list_tokens(&user.id).await {
        Ok(tokens) => tokens,
        Err(e) => {
            tracing::error!("Failed to list tokens: {}", e);
            errors.push(Error {
                title: "Internal error".to_string(),
                message: "Failed to list tokens. Please try again.".to_string(),
                error_type: achtung_ui::error::ErrorType::System,
            });
            vec![]
        }
    };

    pages::settings(&auth_session, user, tokens)
        .with_errors(errors)
        .render()
        .into_response()
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
    let Some(user) = &auth_session.user else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let tokens = state
        .token_manager
        .get_active_tokens(&user.id.clone())
        .await
        .unwrap_or_default();
    let token_name = match TokenName::from_str(&form.name) {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("Invalid token name: {}", e);
            return pages::settings(&auth_session, user, tokens)
                .with_errors(vec![Error {
                    title: "Invalid innput".to_string(),
                    message: "Invalid token name".to_string(),
                    error_type: achtung_ui::error::ErrorType::Validation,
                }])
                .render()
                .into_response();
        }
    };

    match state
        .token_manager
        .create_token(&user.id, &token_name)
        .await
    {
        Ok(plaintext_token) => pages::token_created(user.id, plaintext_token.into(), &auth_session)
            .render()
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to create token: {}", e);

            pages::settings(&auth_session, user, tokens)
                .with_errors(vec![Error {
                    title: "Internal error".to_string(),
                    message: format!("Failed to create token: {}", e).to_string(),
                    error_type: achtung_ui::error::ErrorType::System,
                }])
                .render()
                .into_response()
        }
    }
}

async fn revoke_token(
    auth_session: AuthSession,
    State(state): State<AppState>,
    Path(token_id): Path<i64>,
) -> impl IntoResponse {
    let Some(user) = &auth_session.user else {
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
