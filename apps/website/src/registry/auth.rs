//! Docker Registry Token Authentication Library
//!
//! This library provides Docker Registry v2 token authentication following the spec:
//! https://docs.docker.com/reference/api/registry/auth/

use super::manager::SYSTEM_USERNAME;
use crate::{registry::TokenManager, users::UserId};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rsa::{RsaPublicKey, pkcs8::DecodePrivateKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

#[derive(Debug, Clone)]
pub struct RegistryAuthConfig {
    /// RSA private key in PEM format for signing JWT tokens
    private_key_pem: String,
    pub registry_service: String,
    signing_key: String,
}

impl RegistryAuthConfig {
    pub fn new(
        private_key_pem: String,
        registry_service: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let signing_key = key_id_from_pem(&private_key_pem)?;
        Ok(Self {
            private_key_pem,
            registry_service,
            signing_key,
        })
    }
}

/// Create the registry authentication router
///
/// This is the public Docker Registry v2 token endpoint that doesn't require login.
/// Docker clients call this endpoint to get JWT tokens for registry access.
pub fn router(token_manager: TokenManager, config: RegistryAuthConfig) -> axum::Router {
    use axum::{Router, routing::get};

    Router::new()
        .route("/token", get(token_handler))
        .with_state((token_manager, config))
}

/// Docker registry token auth request parameters
/// https://docs.docker.com/reference/api/registry/auth/
#[derive(Debug, Deserialize)]
pub struct TokenRequest {
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
pub struct TokenResponse {
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
pub struct Access {
    /// Type of resource (e.g., "repository")
    #[serde(rename = "type")]
    resource_type: String,
    /// Resource name (e.g., "user-123/myimage")
    name: String,
    /// Actions allowed (e.g., ["push", "pull"])
    actions: Vec<String>,
}

impl Access {
    pub fn new(resource_type: String, name: String, actions: Vec<String>) -> Self {
        Self {
            resource_type,
            name,
            actions,
        }
    }
}

/// Token auth endpoint
///
/// Docker clients call this with Basic auth (username=user-{id}, password=token)
/// Returns a JWT that grants access to the user's namespace
pub async fn token_handler(
    State((token_manager, config)): State<(TokenManager, RegistryAuthConfig)>,
    Query(params): Query<TokenRequest>,
    headers: HeaderMap,
) -> Result<Json<TokenResponse>, Error> {
    info!(
        "Token request: service={}, scope={:?}",
        params.service, params.scope
    );

    // Validate service matches our registry
    if params.service != config.registry_service {
        return Err(Error::InvalidCredentials);
    }
    info!("Service validated: {}", params.service);

    // Extract Basic auth credentials
    let (username, token) = extract_basic_auth(&headers)?;

    let access_grants = match username.trim() {
        SYSTEM_USERNAME => {
            token_manager
                .validate_system_token(&token)
                .await
                .map_err(|e| {
                    warn!("Token validation failed for system: {}", e);
                    Error::InvalidCredentials
                })?;

            RequestedAccess::parse_scopes(params.scope.as_deref().unwrap_or(""))?
                .validate_for_system()
        }
        _ => {
            // Extract user_id from username (format: "user-{id}")
            // TODO: create a type for user id and validate in ::new()
            let user_id = username
                .strip_prefix("user-")
                .and_then(|s| s.parse::<UserId>().ok())
                .ok_or(Error::InvalidCredentials)?;

            // Validate token and get user_id
            info!("Authenticating user: {}", username);
            token_manager
                .validate_token(&user_id, &token)
                .await
                .map_err(|e| {
                    warn!("Token validation failed for user {}: {}", username, e);
                    Error::InvalidCredentials
                })?;

            // Parse and validate requested scopes against user's namespace
            RequestedAccess::parse_scopes(params.scope.as_deref().unwrap_or(""))?
                .validate_for_user(&user_id)
        }
    };

    let jwt = generate_docker_jwt(username, access_grants, params.service, &config)?;
    let token = jwt.value.clone();
    let expires_in_secs = (jwt.expires_at - jwt.issued_at).as_seconds_f32() as i64;
    let issued_at = jwt
        .issued_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    Ok(Json(TokenResponse {
        token: token.clone(),
        access_token: Some(token),
        expires_in: Some(expires_in_secs),
        issued_at: Some(issued_at),
    }))
}

type DockerService = String;
type Username = String;
pub type JwtEncoded = String;

#[derive(Debug, Clone)]
pub struct RegistryJwtToken {
    pub value: JwtEncoded,
    pub issued_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

pub fn generate_docker_jwt(
    username: Username,
    access_grants: ValidatedAccess,
    service: DockerService,
    config: &RegistryAuthConfig,
) -> Result<RegistryJwtToken, Error> {
    // Generate JWT
    let now = OffsetDateTime::now_utc();
    let exp = now + Duration::minutes(30);

    info!("Generating JWT for {}", &username);

    // https://distribution.github.io/distribution/spec/auth/jwt/
    let claims = Claims {
        iss: "registry-auth".to_string(),
        sub: username,
        aud: service,
        exp: exp.unix_timestamp(),
        nbf: now.unix_timestamp(),
        iat: now.unix_timestamp(),
        jti: Uuid::new_v4().to_string(),
        access: access_grants.0,
    };

    // Use RS256 (RSA with SHA-256) for signing
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(config.signing_key.clone());

    let encoding_key =
        EncodingKey::from_rsa_pem(config.private_key_pem.as_bytes()).map_err(|e| {
            error!("Failed to load RSA private key: {}", e);
            Error::TokenGeneration(e)
        })?;

    let token = encode(&header, &claims, &encoding_key)?;

    Ok(RegistryJwtToken {
        value: token,
        issued_at: now,
        expires_at: exp,
    })
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

    let decoded = String::from_utf8(decoded_bytes).map_err(|_| Error::InvalidCredentials)?;

    // Split on first ':'
    let (username, password) = decoded.split_once(':').ok_or(Error::InvalidCredentials)?;

    Ok((username.to_string(), password.to_string()))
}

#[derive(Debug)]
pub struct RequestedAccess(Vec<Access>);

#[derive(Debug)]
pub struct ValidatedAccess(Vec<Access>);

impl RequestedAccess {
    pub fn new(access_request: Vec<Access>) -> Self {
        Self(access_request)
    }

    /// Parse space-delimited scopes and validate against user namespace
    fn parse_scopes(scopes: &str) -> Result<Self, Error> {
        let mut access_request = Vec::new();

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

            access_request.push(Access {
                resource_type: resource_type.to_string(),
                name: name.to_string(),
                actions,
            });
        }

        Ok(RequestedAccess(access_request))
    }

    /// Validate against user namespace. Returns the intersection of requested scopes and allowed scopes
    fn validate_for_user(self, user_id: &UserId) -> ValidatedAccess {
        let user_namespace = format!("user-{}", user_id);
        let access_grants: Vec<_> = self
            .0
            .into_iter()
            .filter(|access| {
                let granted = access.name.starts_with(&format!("{}/", user_namespace));
                if !granted {
                    warn!(
                        "User {} requested access to '{}' which is outside their namespace '{}'",
                        user_id, access.name, user_namespace
                    )
                }
                granted
            })
            .collect();
        ValidatedAccess(access_grants)
    }

    /// Validate system access requests (allows everything for now)
    pub fn validate_for_system(self) -> ValidatedAccess {
        ValidatedAccess(self.0)
    }
}

/// Generate a Docker registry key ID from a PEM-encoded RSA private key.
///
/// This follows the libtrust specification used by Docker Registry:
/// https://github.com/jlhawn/libtrust/blob/master/util.go#L192
///
/// The key ID is generated by:
/// 1. Extracting the public key from the private key
/// 2. DER encoding the public key (PKIX format)
/// 3. Computing SHA256 hash
/// 4. Truncating to 240 bits (30 bytes)
/// 5. Base32 encoding and formatting as colon-separated 4-character groups
///
/// Returns a key ID in the format: ABCD:EFGH:IJKL:MNOP:QRST:UVWX:YZ23:4567:ABCD:EFGH:IJKL:MNOP
pub fn key_id_from_pem(pem: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Parse the PEM to extract the private key, then get the public key
    let private_key = rsa::RsaPrivateKey::from_pkcs8_pem(pem)?;
    let public_key = RsaPublicKey::from(&private_key);

    // Encode the public key in DER format (PKIX/SPKI)
    use rsa::pkcs8::EncodePublicKey;
    let der_bytes = public_key.to_public_key_der()?;

    // Compute SHA256 hash
    let mut hasher = Sha256::new();
    hasher.update(der_bytes.as_bytes());
    let hash = hasher.finalize();

    // Truncate to 240 bits (30 bytes)
    let truncated = &hash[..30];

    // Base32 encode and format
    Ok(key_id_encode(truncated))
}

/// Encode bytes as base32 and format into colon-separated 4-character groups.
///
/// This matches the keyIDEncode function from libtrust:
/// https://github.com/jlhawn/libtrust/blob/master/util.go#L177
pub(crate) fn key_id_encode(bytes: &[u8]) -> String {
    base32::encode(base32::Alphabet::Rfc4648 { padding: false }, bytes)
        .as_bytes()
        .chunks(4)
        .map(std::str::from_utf8)
        .collect::<Result<Vec<&str>, _>>()
        .unwrap()
        .join(":")
}
