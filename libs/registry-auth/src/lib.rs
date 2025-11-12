//! Docker Registry Token Authentication Library
//!
//! This library provides Docker Registry v2 token authentication following the spec:
//! https://docs.docker.com/reference/api/registry/auth/

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("token generation error: {0}")]
    TokenGeneration(#[from] jsonwebtoken::errors::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Error::InvalidCredentials => (StatusCode::UNAUTHORIZED, "Invalid credentials"),
            Error::Database(ref e) => {
                error!("Database error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            Error::TokenGeneration(ref e) => {
                error!("Token generation error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };
        (status, message).into_response()
    }
}

#[derive(Clone)]
pub struct RegistryAuthConfig {
    pub db: PgPool,
    pub jwt_secret: String,
    pub registry_service: String,
}

/// Docker registry token auth request parameters
/// https://docs.docker.com/reference/api/registry/auth/
#[derive(Debug, Deserialize)]
struct TokenRequest {
    /// The service that hosts the resource (e.g., "achtung-registry.fly.dev")
    service: String,
    /// Space-delimited scope(s) (e.g., "repository:user-123/myimage:push,pull repository:user-123/other:pull")
    #[serde(default)]
    scope: Option<String>,
    /// Client ID (optional)
    #[serde(default)]
    client_id: Option<String>,
}

/// Docker registry token response
/// https://docs.docker.com/registry/spec/auth/token/
#[derive(Debug, Serialize)]
struct TokenResponse {
    /// The JWT token
    token: String,
    /// Access token (same as token for compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
    /// Token expiration in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in: Option<i64>,
    /// When the token was issued
    #[serde(skip_serializing_if = "Option::is_none")]
    issued_at: Option<String>,
}

/// JWT claims for Docker registry token
/// https://docs.docker.com/registry/spec/auth/token/#token-format
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// Issuer
    iss: String,
    /// Subject (username)
    sub: String,
    /// Audience (service)
    aud: String,
    /// Expiration time (unix timestamp)
    exp: i64,
    /// Not before (unix timestamp)
    nbf: i64,
    /// Issued at (unix timestamp)
    iat: i64,
    /// JWT ID
    jti: String,
    /// Access permissions
    access: Vec<Access>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Access {
    /// Type of resource (e.g., "repository")
    #[serde(rename = "type")]
    resource_type: String,
    /// Resource name (e.g., "user-123/myimage")
    name: String,
    /// Actions allowed (e.g., ["push", "pull"])
    actions: Vec<String>,
}

/// Database record for registry token
#[derive(Debug, sqlx::FromRow)]
struct RegistryToken {
    user_id: i64,
    token_hash: String,
    revoked_at: Option<time::OffsetDateTime>,
}

/// Token auth endpoint
///
/// Docker clients call this with Basic auth (username=user-{id}, password=token)
/// Returns a JWT that grants access to the user's namespace
async fn token_handler(
    State(config): State<Arc<RegistryAuthConfig>>,
    Query(params): Query<TokenRequest>,
    headers: HeaderMap,
) -> Result<Json<TokenResponse>, Error> {
    info!("Token request: service={}, scope={:?}", params.service, params.scope);

    // Validate service matches our registry
    if params.service != config.registry_service {
        return Err(Error::InvalidCredentials);
    }

    // Extract Basic auth credentials
    let (username, token) = extract_basic_auth(&headers)?;

    // Validate token and get user_id
    let user_id = validate_token(&config.db, &username, &token).await?;

    // Parse and validate requested scopes against user's namespace
    let requested_scopes = params.scope.as_deref().unwrap_or("");
    let access_grants = parse_and_validate_scopes(requested_scopes, user_id)?;

    // Generate JWT
    let now = OffsetDateTime::now_utc();
    let exp = now + Duration::minutes(30);

    let claims = Claims {
        iss: "registry-auth".to_string(),
        sub: username,
        aud: params.service.clone(),
        exp: exp.unix_timestamp(),
        nbf: now.unix_timestamp(),
        iat: now.unix_timestamp(),
        jti: Uuid::new_v4().to_string(),
        access: access_grants,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )?;

    Ok(Json(TokenResponse {
        token: token.clone(),
        access_token: Some(token),
        expires_in: Some(1800), // 30 minutes
        issued_at: Some(now.format(&time::format_description::well_known::Rfc3339).unwrap()),
    }))
}

/// Extract Basic auth credentials from Authorization header
fn extract_basic_auth(headers: &HeaderMap) -> Result<(String, String), Error> {
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(Error::InvalidCredentials)?;

    // Parse "Basic <base64>" format (HTTP Basic Auth standard - RFC 7617)
    let encoded = auth_header
        .strip_prefix("Basic ")
        .ok_or(Error::InvalidCredentials)?;

    // Decode base64
    let decoded_bytes = STANDARD
        .decode(encoded)
        .map_err(|_| Error::InvalidCredentials)?;

    let decoded = String::from_utf8(decoded_bytes)
        .map_err(|_| Error::InvalidCredentials)?;

    // Split on first ':'
    let (username, password) = decoded
        .split_once(':')
        .ok_or(Error::InvalidCredentials)?;

    Ok((username.to_string(), password.to_string()))
}

/// Parse space-delimited scopes and validate against user namespace
fn parse_and_validate_scopes(scopes: &str, user_id: i64) -> Result<Vec<Access>, Error> {
    let user_namespace = format!("user-{}", user_id);
    let mut access_grants = Vec::new();

    // Split on spaces to get individual scopes
    for scope in scopes.split_whitespace() {
        if scope.is_empty() {
            continue;
        }

        // Parse "type:name:actions" format (e.g., "repository:user-123/myimage:push,pull")
        let parts: Vec<&str> = scope.split(':').collect();
        if parts.len() != 3 {
            warn!("Invalid scope format, skipping: {}", scope);
            continue;
        }

        let resource_type = parts[0];
        let name = parts[1];
        let actions: Vec<String> = parts[2].split(',').map(|s| s.to_string()).collect();

        // Validate that the resource name starts with user's namespace
        if !name.starts_with(&format!("{}/", user_namespace)) {
            warn!(
                "User {} requested access to {} which is outside their namespace {}",
                user_id, name, user_namespace
            );
            continue; // Skip unauthorized scope (per spec: return only authorized access)
        }

        access_grants.push(Access {
            resource_type: resource_type.to_string(),
            name: name.to_string(),
            actions,
        });
    }

    Ok(access_grants)
}

/// Validate a registry token from the database
async fn validate_token(
    db: &PgPool,
    username: &str,
    token: &str,
) -> Result<i64, Error> {
    // Extract user_id from username (format: "user-{id}")
    let user_id = username
        .strip_prefix("user-")
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or(Error::InvalidCredentials)?;

    // Query for non-revoked token
    let record: Option<RegistryToken> = sqlx::query_as(
        "SELECT user_id, token_hash, revoked_at
         FROM registry_tokens
         WHERE user_id = $1 AND revoked_at IS NULL
         LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    let Some(db_token) = record else {
        return Err(Error::InvalidCredentials);
    };

    // Verify token hash
    if !bcrypt::verify(token, &db_token.token_hash).unwrap_or(false) {
        return Err(Error::InvalidCredentials);
    }

    Ok(user_id)
}

/// Create a router for Docker registry token authentication
///
/// Mount this router at `/registry` in your application:
///
/// ```rust,ignore
/// let app = Router::new()
///     .nest("/registry", registry_auth::router(config));
/// ```
///
/// This will expose the token endpoint at `/registry/token`.
pub fn router(config: RegistryAuthConfig) -> Router {
    Router::new()
        .route("/token", get(token_handler))
        .with_state(Arc::new(config))
}
