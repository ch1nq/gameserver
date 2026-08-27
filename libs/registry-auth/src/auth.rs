//! Docker Registry v2 Token Authentication
//!
//! This module implements the Docker Registry v2 token authentication specification:
//! <https://docs.docker.com/registry/spec/auth/token/>

use base64::{Engine, engine::general_purpose::STANDARD};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
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
    /// Public key in PEM format, derived from the private key at construction.
    ///
    /// Cached because [`verify_docker_jwt`] needs it per request, and deriving it
    /// means parsing the private key and re-encoding SPKI every time otherwise.
    public_key_pem: String,
}

impl RegistryAuthConfig {
    pub fn new(
        private_key_pem: String,
        registry_service: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let signing_key = key_id_from_pem(&private_key_pem)?;
        let public_key_pem = public_key_pem_from_private(&private_key_pem)?;
        Ok(Self {
            private_key_pem,
            registry_service,
            signing_key,
            public_key_pem,
        })
    }
}

/// Derive the SPKI public key PEM from a PKCS#8 private key PEM.
fn public_key_pem_from_private(pem: &str) -> Result<String, Box<dyn std::error::Error>> {
    use rsa::pkcs8::{EncodePublicKey, LineEnding};
    let private_key = rsa::RsaPrivateKey::from_pkcs8_pem(pem)?;
    let public_key = RsaPublicKey::from(&private_key);
    Ok(public_key.to_public_key_pem(LineEnding::LF)?)
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

    /// Return the presented token verbatim instead of minting a new one.
    ///
    /// For principals that already hold a valid, correctly-scoped registry JWT —
    /// the coordinator's deploy token being the only case today — the right
    /// answer is to hand it straight back. Minting a fresh token from a
    /// requested scope would put scope authority in two places and create the
    /// possibility of amplifying it; echoing makes amplification structurally
    /// impossible, because nothing is minted. The registry still enforces the
    /// JWT's own `access` claim, exactly as it does when the token is used
    /// directly.
    ///
    /// Returning `Some` bypasses scope parsing and `user_has_access` entirely,
    /// so implementations MUST verify the token's signature first.
    ///
    /// Defaults to `None`, i.e. mint as usual.
    async fn passthrough(
        &self,
        _user_id: &Self::UserId,
        _token: &Self::Token,
    ) -> Option<RegistryJwtToken> {
        None
    }
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
    #[serde(default, rename = "client_id")]
    _client_id: Option<String>,
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

/// Issuer stamped into every token we mint, and required of every token we
/// verify.
const TOKEN_ISSUER: &str = "registry-auth";

/// Pin the JWT crypto backend for this process.
///
/// `jsonwebtoken` selects a backend from its crate features and **panics** if it
/// cannot decide — which is exactly what happens here through feature
/// unification: this crate asks for `rust_crypto`, while `oci-client` (pulled in
/// by the microsandbox machine backend) asks for `aws_lc_rs`. With both enabled
/// the default provider's signer and verifier are `panic!` stubs, so *every*
/// mint and verify would abort at runtime.
///
/// Installing one explicitly makes the choice ours rather than the resolver's.
/// Must be called before any `encode`/`decode`; idempotent, and a lost race is
/// harmless because every caller installs the same provider.
fn ensure_crypto_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        // Errs only if something already installed a provider, which is fine.
        let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
    });
}

/// JWT claims for Docker registry token
/// <https://docs.docker.com/registry/spec/auth/token/#token-format>
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Issuer
    pub iss: String,
    /// Subject (username)
    pub sub: String,
    /// Audience (service)
    pub aud: String,
    /// Expiration time (unix timestamp)
    pub exp: i64,
    /// Not before (unix timestamp)
    pub nbf: i64,
    /// Issued at (unix timestamp)
    pub iat: i64,
    /// JWT ID
    pub jti: String,
    /// Access permissions
    pub access: Vec<Access>,
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
    ensure_crypto_provider();

    let now = OffsetDateTime::now_utc();
    let exp = now + Duration::minutes(30);

    info!("Generating JWT for {}", &username);

    // https://distribution.github.io/distribution/spec/auth/jwt/
    let claims = Claims {
        iss: TOKEN_ISSUER.to_string(),
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

/// Verify a JWT we previously issued and return its claims.
///
/// Checks the RS256 signature against our own public key plus `aud`, `iss` and
/// `exp`. Used by the passthrough path: even though the token is only echoed
/// back, verifying it here means the endpoint 401s at the auth layer instead of
/// handing an unvalidated string to a registry client.
pub fn verify_docker_jwt(
    token: &str,
    config: &RegistryAuthConfig,
) -> Result<Claims, RegistryAuthError> {
    ensure_crypto_provider();

    let decoding_key = DecodingKey::from_rsa_pem(config.public_key_pem.as_bytes()).map_err(|e| {
        error!("Failed to load RSA public key: {}", e);
        RegistryAuthError::TokenGeneration
    })?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[config.registry_service.as_str()]);
    validation.set_issuer(&[TOKEN_ISSUER]);
    // `exp` is validated by default; require it (and `aud`/`iss`) to be present
    // so a token omitting a claim cannot skip the corresponding check.
    validation.set_required_spec_claims(&["exp", "aud", "iss"]);

    decode::<Claims>(token, &decoding_key, &validation)
        .map(|data| data.claims)
        .map_err(|e| {
            warn!("JWT verification failed: {}", e);
            RegistryAuthError::InvalidCredentials
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
///
/// `Sync` is required because [`RegistryAuth::passthrough`] has a default body,
/// which `async_trait` desugars into a future holding `&self` across an await.
/// [`router`] already demands it.
#[cfg(feature = "axum-integration")]
pub async fn token_handler<R: RegistryAuth + Sync>(
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

    // A principal that already holds a valid, correctly-scoped registry JWT gets
    // it back unchanged. Checked before scope parsing: the requested scope plays
    // no part, because nothing is minted from it.
    let jwt = match registry_auth.passthrough(&user_id, &token).await {
        Some(jwt) => {
            info!("Returning presented token unchanged for {}", &username);
            jwt
        }
        None => {
            let scope_str = params.scope.join(" ");
            let reqeusted_access = RequestedAccess::parse_scopes(&scope_str)?;
            let access_grants = reqeusted_access.validate_for_user::<R>(&user_id);
            generate_docker_jwt::<R>(username, access_grants, params.service, &config)?
        }
    };

    let token = jwt.value.clone();
    // Derived from the token's own lifetime rather than a fixed window, so a
    // passed-through token is not cached by the client past its real `exp`.
    let expires_in_secs = (jwt.expires_at - OffsetDateTime::now_utc())
        .as_seconds_f32()
        .max(0.0) as i64;
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

    /// Test RSA key standing in for `REGISTRY_PRIVATE_KEY`.
    const TEST_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
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

    /// A different RSA key, used to prove a token signed by someone else is
    /// rejected rather than merely parsed.
    const FOREIGN_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDmtRli54T5csg3
FJTHsb9U2dZd8cOt/pOmhZuWcF6BRwyiKIhPnxcwI9XRm3F6x/Qq1rt/9k0gPzlQ
AigTYDCrz7XM8dewFFuZWbtasXpEc03y1Oh44ZM4urJy1BOav+Zu4FMEcw2xAv+e
xyR/lqpysNM2B7lysa0FQqCQ32NWfkb3NzcG6SN/vX9ZJ5GMvX/a3BZt0DHxZeOk
Z46JMVx3UuUe2Oj1HQHIvqGOLPuz+3qRglL6tJR4sB5TWlcsA6MsgqsCf1BppZh0
VvpU7RzSBkQnsEtAsk0p5s40NrkIlq5c6Rwg+PTETV2RaM7ChlR1oSrGSdNk4lFZ
TEXlAaOzAgMBAAECggEAGYzQ5O0zAtU9aywyVfNPdzwwy3Ks8yYQgA6n7n8/WB3g
Pk0y226JCOHPGkmWxbxDRENHvKIwZHPcCwpSGeM7QKvePHZEJtH6Wv9fCmpBWjdS
2KPPoyOIRG4YuTLXgPnjsT/Ssdl0GLh2SsVPO3oaIl2G5qLwXM1klgKM+b5jp/5a
UOGO4WLFHpcUR+7DO93KxtOoh2/l4v+LtdblSiJj7UhNAt/J3bGSEj0mU+IwDK/A
38iLyIXmKFYZRp5LDCsdQCucc2Uq+DINiI6vJ1AwQTyUiKdfkuhKIwuAn8TDBdQI
eULrO7ICQq79DpoxTCEssSwa9eMt4nco/lA6IqECwQKBgQD5TbsQpNqQcz6v4Hv/
iuwi/jKZQ9o0tmwxC0NGk9aWuEpF8eN0Q+XnPUSSL8u4riTpgiXP7yqtmlHItVtH
UR1Ap71JEJjEGMzQFVWCVdJ376TmitGYahSMq0XbM2rXwiej+6mY8cR+2mrcaJlM
T3omoUOjkewhLecNTIIOW5LscQKBgQDs538fA2aYFHWt/5T0mpPTomDaPejSTweR
0UW/xj5FXqoDcjQkbsikUl1UQKnIP1Zn4RsQO/E5HQBikl85P1jleK2EZ6Zpa2fI
WX7ndEyIec5u2izID9TPHtggGw9ckC8wSUjyntSYLErGqJcULfgvuNNUfAhY64jL
Ood3s110YwKBgQDhPkWhSBDhKf6dUSk3PQEUrK5yo0dnENq3hQGHptLe4irY/y8O
QLpbLpPhsKVTeqOHBju7ns7kguUZfiG2UacoX2U5unEL24xRBLV5SKkcC7zlPs8X
8eAXKDe5UL9bqOO/2QTmVqm+IwEhmq/Grpgihtlh09mQMLTs4w8ugbZBQQKBgBni
vbAs1fP+IFGv4J3NmiOA1aZjJ2J7gi87t6xZxAoeauNPgkUM2d2iplIDcsnPqehV
33gppJUCBz2+EquVsWf5hLQ4AyX3t3Jb3RL7UTWEYbsZGdWObUlobGMtscMCejWD
fHYORtqN1GnamA97amgEgQr1NpBIxDy4m37H2YlTAoGBAIH1eyLJLp14UvfH4Y2a
ptH7fyhDAUA0VLiQ4qkHYu2Ey8nB4Ndn35Wf4cIwiHHmLBPekQUD1F8Lnih6Nztr
wrS/1JT56ML2E1SrwRn0BQG7cyEgDIROBWYTPgEd14vFkNLGnIWQYIXOgqpZtMbs
8q9WLtwktrTM5Z2h6r+DsXOH
-----END PRIVATE KEY-----"#;

    const TEST_SERVICE: &str = "registry:5001";

    fn test_config(pem: &str) -> RegistryAuthConfig {
        RegistryAuthConfig::new(pem.to_string(), TEST_SERVICE.to_string())
            .expect("config builds from a valid PKCS#8 key")
    }

    /// Mint a token with explicit lifetime bounds, bypassing
    /// `generate_docker_jwt`'s fixed 30-minute window so expiry can be tested.
    fn mint_with_lifetime(
        config: &RegistryAuthConfig,
        access: Vec<Access>,
        iat: OffsetDateTime,
        exp: OffsetDateTime,
    ) -> String {
        let claims = Claims {
            iss: TOKEN_ISSUER.to_string(),
            sub: "system".to_string(),
            aud: config.registry_service.clone(),
            exp: exp.unix_timestamp(),
            nbf: iat.unix_timestamp(),
            iat: iat.unix_timestamp(),
            jti: Uuid::new_v4().to_string(),
            access,
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(config.signing_key.clone());
        let key = EncodingKey::from_rsa_pem(config.private_key_pem.as_bytes()).unwrap();
        encode(&header, &claims, &key).unwrap()
    }

    fn pull_access(repository: &str) -> Vec<Access> {
        vec![Access::new(
            "repository".to_string(),
            repository.to_string(),
            vec!["pull".to_string()],
        )]
    }

    #[test]
    fn verify_accepts_our_own_token_with_scope_intact() {
        let config = test_config(TEST_PEM);
        let jwt = generate_docker_jwt::<TestRegistryAuth>(
            "system".to_string(),
            ValidatedAccess::new(pull_access("user-5/bot")),
            TEST_SERVICE.to_string(),
            &config,
        )
        .expect("mint succeeds");

        let claims = verify_docker_jwt(&jwt.value, &config).expect("our own token verifies");

        // The access claim is what the registry enforces, so passthrough is only
        // safe if it survives the round trip unchanged.
        assert_eq!(claims.access.len(), 1);
        assert_eq!(claims.access[0].name, "user-5/bot");
        assert_eq!(claims.access[0].actions, vec!["pull"]);
        assert_eq!(claims.sub, "system");
    }

    #[test]
    fn verify_rejects_an_expired_token() {
        let config = test_config(TEST_PEM);
        let now = OffsetDateTime::now_utc();
        // Well past the 60s default leeway.
        let token = mint_with_lifetime(
            &config,
            pull_access("user-5/bot"),
            now - Duration::hours(2),
            now - Duration::hours(1),
        );

        assert!(
            verify_docker_jwt(&token, &config).is_err(),
            "an expired token must not pass through"
        );
    }

    #[test]
    fn verify_rejects_a_token_signed_by_another_key() {
        let ours = test_config(TEST_PEM);
        let theirs = test_config(FOREIGN_PEM);
        let now = OffsetDateTime::now_utc();

        // Correct issuer, audience and scope — only the signature is foreign.
        let token = mint_with_lifetime(
            &theirs,
            pull_access("user-5/bot"),
            now,
            now + Duration::minutes(30),
        );

        assert!(
            verify_docker_jwt(&token, &ours).is_err(),
            "a token we did not sign must be rejected"
        );
    }

    #[test]
    fn verify_rejects_a_token_for_another_service() {
        let config = test_config(TEST_PEM);
        let other_service =
            RegistryAuthConfig::new(TEST_PEM.to_string(), "someone-else:5001".to_string()).unwrap();
        let now = OffsetDateTime::now_utc();

        // Same key, different `aud`: a token minted for another registry must not
        // be replayable against ours.
        let token = mint_with_lifetime(
            &other_service,
            pull_access("user-5/bot"),
            now,
            now + Duration::minutes(30),
        );

        assert!(verify_docker_jwt(&token, &config).is_err());
    }

    #[test]
    fn passthrough_defaults_to_minting() {
        // The default impl keeps every existing implementor on the mint path, so
        // adding the method cannot silently change behaviour.
        let auth = TestRegistryAuth;
        let result = futures_lite_block_on(auth.passthrough(&TestUserId(1), &String::new()));
        assert!(result.is_none());
    }

    /// Minimal executor: this crate has no async runtime dependency, and the
    /// default `passthrough` body never actually awaits.
    fn futures_lite_block_on<F: std::future::Future>(fut: F) -> F::Output {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
        fn noop(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut fut = Box::pin(fut);
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(out) => out,
            Poll::Pending => panic!("the default passthrough impl must not await"),
        }
    }

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
