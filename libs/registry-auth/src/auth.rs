//! Docker Registry v2 Token Authentication
//!
//! This module implements the Docker Registry v2 token authentication specification:
//! <https://docs.docker.com/registry/spec/auth/token/>

use base64::{Engine, engine::general_purpose::STANDARD};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rsa::{RsaPublicKey, pkcs8::DecodePrivateKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::str::FromStr;
use time::{Duration, OffsetDateTime};
use tracing::{error, info, warn};
use uuid::Uuid;

#[cfg(feature = "axum-integration")]
use axum::{http::StatusCode, response::IntoResponse};

/// Configuration for Docker registry authentication
#[derive(Debug, Clone)]
pub struct RegistryAuthConfig {
    /// RSA private key in PEM format for signing JWT tokens
    private_key_pem: String,
    /// Registry service name (e.g., "achtung-registry.fly.dev")
    pub registry_service: String,
    /// Key ID for JWT header (derived from public key)
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

/// Error type for token storage operations
#[derive(Debug, thiserror::Error)]
pub enum RegistryAuthError {
    #[error("Invalid scope: {0}")]
    InvalidScope(String),

    #[error("Failed to generate token")]
    TokenGeneration,

    #[error("Failed to extract auth headers")]
    ExtractAuthHeader,

    #[error("Invalid credentials")]
    InvalidCredentials,
}

#[cfg(feature = "axum-integration")]
impl IntoResponse for RegistryAuthError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            RegistryAuthError::ExtractAuthHeader => StatusCode::UNAUTHORIZED,
            RegistryAuthError::InvalidScope(_) => StatusCode::UNAUTHORIZED,
            RegistryAuthError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            RegistryAuthError::TokenGeneration => StatusCode::INTERNAL_SERVER_ERROR,
        };
        status.into_response()
    }
}

type Username = String;

#[async_trait::async_trait]
pub trait RegistryAuth {
    type UserId;
    type Token: FromStr;

    /// Map a username to a user id. E.g. "@johnsmith" -> 1337
    fn parse_user_id(username: Username) -> Option<Self::UserId>;

    /// Validate registry access request for a user
    fn user_has_access(access: &Access, user_id: &Self::UserId) -> bool;

    /// Validate a user's token
    async fn is_valid_token(&self, user_id: &Self::UserId, token: &Self::Token) -> bool;
}

/// Docker registry JWT token with metadata
#[derive(Debug, Clone)]
pub struct RegistryJwtToken {
    pub value: String,
    pub issued_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

/// Docker registry token auth request parameters
/// <https://docs.docker.com/reference/api/registry/auth/>
#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    /// The service that hosts the resource (e.g., "achtung-registry.fly.dev")
    service: String,
    /// Scope(s) for registry access. Can be specified multiple times in the query string.
    /// Each scope has format "type:name:actions" (e.g., "repository:user-123/myimage:push,pull")
    #[serde(default)]
    scope: Vec<String>,
    /// Client ID (optional)
    #[serde(default)]
    client_id: Option<String>,
}

/// Docker registry token response
/// <https://docs.docker.com/registry/spec/auth/token/>
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
/// <https://docs.docker.com/registry/spec/auth/token/#token-format>
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

/// Access grant for a Docker registry resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Access {
    /// Type of resource (e.g., "repository")
    #[serde(rename = "type")]
    pub resource_type: String,
    /// Resource name (e.g., "user-123/myimage")
    pub name: String,
    /// Actions allowed (e.g., ["push", "pull"])
    pub actions: Vec<String>,
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

/// Requested access scopes (before validation)
#[derive(Debug)]
pub struct RequestedAccess(Vec<Access>);

/// Validated access scopes (after namespace validation)
#[derive(Debug)]
pub struct ValidatedAccess(Vec<Access>);

impl RequestedAccess {
    pub fn new(access_request: Vec<Access>) -> Self {
        Self(access_request)
    }

    /// Parse space-delimited scopes
    /// Format: "type:name:actions" e.g., "repository:user-123/myimage:push,pull"
    pub fn parse_scopes(scopes: &str) -> Result<Self, RegistryAuthError> {
        let mut access_request = Vec::new();

        for scope in scopes.split_whitespace() {
            if scope.is_empty() {
                continue;
            }

            let parts: Vec<&str> = scope.split(':').collect();
            if parts.len() != 3 {
                warn!("Invalid scope format, skipping: {}", scope);
                return Err(RegistryAuthError::InvalidScope(scope.to_string()));
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

    /// Validate scopes against user namespace
    /// Returns only the scopes that are within the user's namespace
    pub fn validate_for_user<R: RegistryAuth>(self, user_id: &R::UserId) -> ValidatedAccess {
        let access_grants: Vec<_> = self
            .0
            .into_iter()
            .filter(|access| R::user_has_access(access, user_id))
            .collect();
        ValidatedAccess(access_grants)
    }
}

impl ValidatedAccess {
    /// Create a new ValidatedAccess. Only use this if you are sure that the access grants are actually valid.
    /// Otherwise, please use `RequestedAccess::validate_for_user`
    pub fn new(access_grants: Vec<Access>) -> Self {
        ValidatedAccess(access_grants)
    }
}

/// Generate a Docker registry JWT token
pub fn generate_docker_jwt<R: RegistryAuth>(
    username: Username,
    access_grants: ValidatedAccess,
    service: String,
    config: &RegistryAuthConfig,
) -> Result<RegistryJwtToken, RegistryAuthError> {
    let now = OffsetDateTime::now_utc();
    let exp = now + Duration::minutes(30);

    info!("Generating JWT for {}", &username);

    // https://distribution.github.io/distribution/spec/auth/jwt/
    let claims = Claims {
        iss: "registry-auth".to_string(),
        sub: username.to_string(),
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
            RegistryAuthError::TokenGeneration
        })?;

    let token =
        encode(&header, &claims, &encoding_key).map_err(|_| RegistryAuthError::TokenGeneration)?;

    Ok(RegistryJwtToken {
        value: token,
        issued_at: now,
        expires_at: exp,
    })
}

/// Extract Basic auth credentials from Authorization header
#[cfg(feature = "axum-integration")]
fn extract_basic_auth(
    headers: &axum::http::HeaderMap,
) -> Result<(Username, String), RegistryAuthError> {
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(RegistryAuthError::ExtractAuthHeader)?;

    let encoded = auth_header
        .strip_prefix("Basic ")
        .ok_or(RegistryAuthError::ExtractAuthHeader)?;

    let decoded_bytes = STANDARD
        .decode(encoded)
        .map_err(|_| RegistryAuthError::ExtractAuthHeader)?;

    let decoded =
        String::from_utf8(decoded_bytes).map_err(|_| RegistryAuthError::ExtractAuthHeader)?;

    let (username, password) = decoded
        .split_once(':')
        .ok_or(RegistryAuthError::ExtractAuthHeader)?;

    Ok((username.to_string(), password.to_string()))
}

/// Token auth handler for axum
#[cfg(feature = "axum-integration")]
pub async fn token_handler<R: RegistryAuth>(
    axum::extract::State((registry_auth, config)): axum::extract::State<(R, RegistryAuthConfig)>,
    axum_extra::extract::Query(params): axum_extra::extract::Query<TokenRequest>,
    headers: axum::http::HeaderMap,
) -> Result<axum::Json<TokenResponse>, RegistryAuthError> {
    info!(
        "Token request: service={}, scope={:?}",
        params.service, params.scope
    );

    // Validate service matches our registry
    if params.service != config.registry_service {
        return Err(RegistryAuthError::InvalidCredentials);
    }
    info!("Service validated: {}", params.service);

    // Extract Basic auth credentials
    let (username, token) = extract_basic_auth(&headers)?;
    let token = token
        .parse::<R::Token>()
        .map_err(|_| RegistryAuthError::InvalidCredentials)?;
    let user_id =
        R::parse_user_id(username.clone()).ok_or(RegistryAuthError::InvalidCredentials)?;

    info!("Authenticating user: {}", &username);
    if !registry_auth.is_valid_token(&user_id, &token).await {
        warn!("Token validation failed for user {}", username);
        return Err(RegistryAuthError::InvalidCredentials);
    }

    let scope_str = params.scope.join(" ");
    let reqeusted_access = RequestedAccess::parse_scopes(&scope_str)?;
    let access_grants = reqeusted_access.validate_for_user::<R>(&user_id);

    let jwt = generate_docker_jwt::<R>(username, access_grants, params.service, &config)?;
    let token = jwt.value.clone();
    let expires_in_secs = (jwt.expires_at - jwt.issued_at).as_seconds_f32() as i64;
    let issued_at = jwt
        .issued_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    Ok(axum::Json(TokenResponse {
        token: token.clone(),
        access_token: Some(token),
        expires_in: Some(expires_in_secs),
        issued_at: Some(issued_at),
    }))
}

/// Create the registry authentication router
///
/// This is the public Docker Registry v2 token endpoint.
/// Docker clients call this endpoint to get JWT tokens for registry access.
#[cfg(feature = "axum-integration")]
pub fn router<R>(registry_auth: R, config: RegistryAuthConfig) -> axum::Router
where
    R: RegistryAuth + Send + Sync + Clone + 'static,
    R::UserId: Send,
    R::Token: Send,
{
    use axum::{Router, routing::get};

    Router::new()
        .route("/token", get(token_handler))
        .with_state((registry_auth, config))
}

/// Generate a Docker registry key ID from a PEM-encoded RSA private key.
///
/// This follows the libtrust specification used by Docker Registry:
/// <https://github.com/jlhawn/libtrust/blob/master/util.go#L192>
///
/// The key ID is generated by:
/// 1. Extracting the public key from the private key
/// 2. DER encoding the public key (PKIX format)
/// 3. Computing SHA256 hash
/// 4. Truncating to 240 bits (30 bytes)
/// 5. Base32 encoding and formatting as colon-separated 4-character groups
pub fn key_id_from_pem(pem: &str) -> Result<String, Box<dyn std::error::Error>> {
    let private_key = rsa::RsaPrivateKey::from_pkcs8_pem(pem)?;
    let public_key = RsaPublicKey::from(&private_key);

    use rsa::pkcs8::EncodePublicKey;
    let der_bytes = public_key.to_public_key_der()?;

    let mut hasher = Sha256::new();
    hasher.update(der_bytes.as_bytes());
    let hash = hasher.finalize();

    // Truncate to 240 bits (30 bytes)
    let truncated = &hash[..30];

    Ok(key_id_encode(truncated))
}

/// Encode bytes as base32 and format into colon-separated 4-character groups
fn key_id_encode(bytes: &[u8]) -> String {
    base32::encode(base32::Alphabet::Rfc4648 { padding: false }, bytes)
        .as_bytes()
        .chunks(4)
        .map(std::str::from_utf8)
        .collect::<Result<Vec<&str>, _>>()
        .unwrap()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_scopes() {
        let scopes = "repository:user-123/myimage:push,pull repository:user-123/other:pull";
        let requested = RequestedAccess::parse_scopes(scopes).unwrap();
        assert_eq!(requested.0.len(), 2);
        assert_eq!(requested.0[0].name, "user-123/myimage");
        assert_eq!(requested.0[0].actions, vec!["push", "pull"]);
    }

    struct TestUserId(u32);
    impl TryFrom<Username> for TestUserId {
        type Error = String;

        fn try_from(value: Username) -> Result<Self, Self::Error> {
            let id = value
                .strip_prefix("user-")
                .ok_or("Failed to strip prefix")?
                .parse::<u32>()
                .map_err(|_| "failed to parse user id")?;
            Ok(TestUserId(id))
        }
    }

    struct TestRegistryAuth;

    #[async_trait::async_trait]
    impl RegistryAuth for TestRegistryAuth {
        type UserId = TestUserId;
        type Token = String;

        fn user_has_access(access: &Access, user_id: &Self::UserId) -> bool {
            access.name.starts_with(&format!("user-{}/", user_id.0))
        }

        async fn is_valid_token(&self, _user_id: &Self::UserId, _token: &Self::Token) -> bool {
            unreachable!()
        }

        fn parse_user_id(_username: Username) -> Option<Self::UserId> {
            unreachable!()
        }
    }

    #[test]
    fn test_validate_user_namespace() {
        let access = vec![
            Access::new(
                "repository".to_string(),
                "user-123/allowed".to_string(),
                vec!["push".to_string()],
            ),
            Access::new(
                "repository".to_string(),
                "user-456/blocked".to_string(),
                vec!["pull".to_string()],
            ),
        ];
        let requested = RequestedAccess(access);

        let validated = requested.validate_for_user::<TestRegistryAuth>(&TestUserId(123));

        assert_eq!(validated.0.len(), 1);
        assert_eq!(validated.0[0].name, "user-123/allowed");
    }

    #[test]
    fn test_key_id_format() {
        // Use a valid RSA private key for testing
        let test_pem = r#"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC2RrLNE/QKgneY
QpyNcFuEkIpdMWOHMPXAbPZc0ypBY1COCU7Dx3rVT0Sn7UsZE/fwYImxTMUtp6sz
5MTPr6QpmwZbAJyYUbId2SbxT2jORKYSdtqc1aySAdrUdsQxaB/xhmIwkWRk6ZTI
tw6Uf6lktaLBS2QL3/+z55k1iMs+w+FlKu1TfLArPT6UllzWzOgSvaxOTnWw5IPl
c77MiDm+YF3eO9FKHkC4l2ftZEEM2lXxuFwFrHqNm7BKjuwkzWHm1ARLghBH0KQZ
N8p3ysExS1dyziOJKBdAZNplaK9zGRJLaUU71nNNQKjFbwtMd/KqER9RTZYfKEMJ
pveg4OYPAgMBAAECggEAHNcj3Fn/X5hUFvXXMnPoLxn1opg5cL8Y60jyVC6fPXha
2xZy7XxHHbAso0ti+gVUUibcMn78peQlLRFR6LCYT3L1dvmqTVmDzsA4rq7LXPO0
uTAwF+ehJfsAJmTiVxTsFPmX2KpwkZz5yyZXurxWT5aDuYTVwCFBorQO5E8QJY5w
/D/7qvdkMgmdyXjW+d6eApBmj8Wue/hq3QXCVVsTgA/FDVPPUfH52vx/O8ABhT5+
VtTRZqiQYCkuVrGIJ0qStp/W99XOeHAn02/UIoMh1a4G2LkZY+VP8wttE6KrZ1VW
hBTbvBWwMqAPP7gIYecScbPjXclW3GbmtzaASmr4lQKBgQDw3Y5lmPoxpAmHaGbA
n/IZRTTh1qMXWX1+s+FXhfsuGEdrt48aUfEPs3erIcSXD/ExCx8pDq8tB6GQe+ZO
bKUsONh+f0gZxM+37V9K/bvp0MtGAXzcDuvcBPB79N+8F9pwdZNa2UG44kEMgzyd
E1mzReCe0+Phywb0XHAyP6gM7QKBgQDBurQfFAndoJLHuTyMQsOnVcBcKH1bQ5fI
Y5xq+dX9NyTUjEsCWOiG/wRzuc4378B05L4zSUymBgTTj+fO6gVTYvFTBePrH+da
ERFmyv2Dpyj+YKRpm8TFYFQvdQv3vQoTWgqz3Q8ZPGsqdA8y1pcfcEc8107zmPQD
wjrxcxCbawKBgQCDs/HX1dUAbbyUIN8Gdq7PaIso7c8RxmobbMpLrEQTCU2MNbt2
3dVdC3nkxjsTirEMaxNnxNK+YYzTTxw4R6ntS0v9pyVKidY2sQHJJIKqr/NmXQvj
2/jVvpGshdIMrFJR6chgBamtKXH+IIh1Lw5+Ozg+QIg7f2NXHHBw2WPPZQKBgDR1
K+Tmdi1vF4/BVuXcBkK/c5EA3cDisqzuXCKTeCBS2EQ9oOoHzR8Q2tHDVFXNM93z
OpWEmZ6zLodjBi//KmYD+riydZ7rSqgWyxF8kd0eXHlVDfAS39taVDFtjkoNBDdt
QEyn5Ti+JX6fYqYveUhoDMIqwxQvLJP/+hn7QFn1AoGBAOcyh1axbKVGvQfN5LUL
Ub7SGmN8Bo8nweJQwVN++HkuJgA1qeFSAmHkTb5SWvlLo5SGnCggJOBHS2YdsWBI
6kQxb6WosnoGl3DIp3QlWTJ0KTc5zgH5ufDzUsjCf6Kixm46T00gNXxAL4394uB2
hgvjlUMEsLIcj8xxegi/k4iQ
-----END PRIVATE KEY-----"#;

        let key_id = key_id_from_pem(test_pem).expect("Failed to generate key ID");

        // Verify the format: 12 groups of 4 characters separated by colons
        let parts: Vec<&str> = key_id.split(':').collect();
        assert_eq!(
            parts.len(),
            12,
            "Key ID should have 12 colon-separated groups"
        );

        for (i, part) in parts.iter().enumerate() {
            assert_eq!(
                part.len(),
                4,
                "Group {} should have 4 characters, got: {}",
                i,
                part
            );

            // Verify all characters are valid base32 (A-Z, 2-7)
            for ch in part.chars() {
                assert!(
                    ch.is_ascii_uppercase() || ('2'..='7').contains(&ch),
                    "Invalid base32 character: {}",
                    ch
                );
            }
        }

        println!("Generated key ID: {}", key_id);
    }
}
