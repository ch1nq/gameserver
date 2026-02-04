use super::token::{PlaintextToken, RegistryToken, TokenName};
use crate::users::UserId;
use registry_auth::auth::{Access, RegistryAuth, ValidatedAccess};
use registry_auth::{RegistryAuthConfig, RegistryJwtToken};
use sqlx::PgPool;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct RegistryTokenManager {
    db_pool: PgPool,
    system_token: Arc<RwLock<Option<RegistryJwtToken>>>,
    registry_auth_config: RegistryAuthConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum TokenManagerError {
    #[error("Database error: {0}")]
    DatabaseError(sqlx::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Token limit reached")]
    TokenLimitReached,

    #[error("Token not found")]
    TokenNotFound,

    #[error("Failed to generate system token")]
    FailedToGenerateSystemToken,

    #[error("Failed to hash token: {0}")]
    FailedToHashToken(String),

    #[error("Invalid credentials")]
    InvalidCredentials,
}

const MAX_TOKENS_PER_USER: i64 = 10;
const BCRYPT_COST: u32 = 12;
const SYSTEM_USERNAME: &str = "system";

impl RegistryTokenManager {
    pub fn new(db_pool: PgPool, registry_auth_config: RegistryAuthConfig) -> Self {
        Self {
            db_pool,
            system_token: Arc::new(RwLock::new(None)),
            registry_auth_config,
        }
    }

    /// Create a new registry token for a user
    /// Returns the token ID and the plaintext token (only time it's visible)
    pub async fn create_token(
        &self,
        user_id: &UserId,
        name: &TokenName,
    ) -> Result<PlaintextToken, TokenManagerError> {
        // Check token limit
        let count = self.count_active_tokens(user_id).await?;
        if count >= MAX_TOKENS_PER_USER {
            return Err(TokenManagerError::TokenLimitReached);
        }

        // Generate plaintext token
        let plaintext_token = PlaintextToken::generate();

        // Hash the token using bcrypt
        let token_hash = bcrypt::hash(plaintext_token.as_ref(), BCRYPT_COST)
            .map_err(|e| TokenManagerError::FailedToHashToken(e.to_string()))?;

        // Insert into database
        let _token_id = sqlx::query!(
            r#"
            INSERT INTO registry_tokens (user_id, token_hash, name)
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
            user_id,
            token_hash,
            name.as_ref(),
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(TokenManagerError::DatabaseError)?
        .id;

        Ok(plaintext_token)
    }

    /// Get or create a system token for this website instance. This token is
    /// cached in memory and reused across requests. Returns the plaintext token
    /// that can be used for registry authentication
    pub async fn get_system_token(&self) -> Result<RegistryJwtToken, TokenManagerError> {
        // Check if we have a valid cached token with enough time remaining
        {
            let guard = self.system_token.read().await;
            if let Some(sys_token) = guard.as_ref() {
                // Check database to see if token has at least 5 minutes remaining
                if sys_token.expires_at > OffsetDateTime::now_utc() + Duration::minutes(5) {
                    tracing::debug!("Reusing cached system token");
                    return Ok(sys_token.clone());
                }
                tracing::debug!("Cached token expiring soon");
            }
        }

        tracing::debug!("Generating new token");

        let access_grants = ValidatedAccess::new(vec![Access::new(
            "registry".to_string(),
            "catalog".to_string(),
            vec!["*".to_string()],
        )]);

        let jwt = registry_auth::auth::generate_docker_jwt::<Self>(
            SYSTEM_USERNAME.to_string(),
            access_grants,
            self.registry_auth_config.registry_service.clone(),
            &self.registry_auth_config,
        )
        .map_err(|_| TokenManagerError::FailedToGenerateSystemToken)?;

        // Cache the plaintext token
        let mut guard = self.system_token.write().await;
        *guard = Some(jwt.clone());

        Ok(jwt)
    }

    /// Create a new JWT token that has pull access to the given image repository
    pub async fn get_system_deploy_token_for(
        &self,
        repository: &str,
    ) -> Result<RegistryJwtToken, TokenManagerError> {
        let access_grants = ValidatedAccess::new(vec![Access::new(
            "repository".to_string(),
            repository.to_string(),
            vec!["pull".to_string()],
        )]);

        let jwt = registry_auth::auth::generate_docker_jwt::<Self>(
            SYSTEM_USERNAME.to_string(),
            access_grants,
            self.registry_auth_config.registry_service.clone(),
            &self.registry_auth_config,
        )
        .map_err(|_| TokenManagerError::FailedToGenerateSystemToken)?;

        tracing::info!(
            "Generated deploy token for repository '{}', expires at {}",
            repository,
            jwt.expires_at
        );

        Ok(jwt)
    }

    /// List all active (non-revoked) tokens for a user
    pub async fn list_tokens(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<RegistryToken>, TokenManagerError> {
        let tokens = sqlx::query_as!(
            RegistryToken,
            r#"
            SELECT id, user_id, name, token_hash, created_at, revoked_at
            FROM registry_tokens
            WHERE user_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(TokenManagerError::DatabaseError)?;

        Ok(tokens)
    }

    /// Revoke a token (soft delete by setting revoked_at)
    pub async fn revoke_token(
        &self,
        user_id: &UserId,
        token_id: i64,
    ) -> Result<(), TokenManagerError> {
        let result = sqlx::query!(
            r#"
            UPDATE registry_tokens
            SET revoked_at = NOW()
            WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL
            "#,
            token_id,
            user_id,
        )
        .execute(&self.db_pool)
        .await
        .map_err(TokenManagerError::DatabaseError)?;

        if result.rows_affected() == 0 {
            return Err(TokenManagerError::TokenNotFound);
        }

        Ok(())
    }

    /// Count active tokens for a user
    pub async fn count_active_tokens(&self, user_id: &UserId) -> Result<i64, TokenManagerError> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as "count!"
            FROM registry_tokens
            WHERE user_id = $1 AND revoked_at IS NULL
            "#,
            user_id
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(TokenManagerError::DatabaseError)?
        .count;

        Ok(count)
    }

    pub async fn get_active_tokens(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<RegistryToken>, TokenManagerError> {
        sqlx::query_as!(
            RegistryToken,
            r#"
            SELECT id, user_id, name, token_hash, created_at, revoked_at
            FROM registry_tokens
            WHERE user_id = $1 AND revoked_at IS NULL
            "#,
            user_id
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(TokenManagerError::DatabaseError)
    }

    /// Validate a registry token for a user
    pub async fn validate_token(
        &self,
        user_id: &UserId,
        token_plaintext: &str,
    ) -> Result<(), TokenManagerError> {
        let candidates = self.get_active_tokens(user_id).await?;
        for candidate in candidates {
            if bcrypt::verify(token_plaintext, &candidate.token_hash).unwrap_or(false) {
                return Ok(());
            }
        }

        Err(TokenManagerError::InvalidCredentials)
    }
}

#[async_trait::async_trait]
impl RegistryAuth for RegistryTokenManager {
    type UserId = UserId;
    type Token = String;

    fn parse_user_id(username: String) -> Option<UserId> {
        username
            .strip_prefix("user-")
            .map(|id| id.parse::<UserId>().ok())
            .flatten()
    }

    fn user_has_access(access: &Access, user_id: &UserId) -> bool {
        let user_namespace = format!("user-{}", user_id);
        let granted = access.name.starts_with(&format!("{}/", user_namespace));
        if !granted {
            tracing::warn!(
                "User {} requested access to '{}' which is outside their namespace '{}'",
                user_id,
                access.name,
                user_namespace
            )
        }
        granted
    }

    async fn is_valid_token(&self, user_id: &UserId, token: &Self::Token) -> bool {
        self.validate_token(user_id, token).await.is_ok()
    }
}
