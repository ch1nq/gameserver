use crate::users::UserId;

use super::token::{PlaintextToken, RegistryToken, TokenName};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct TokenManager {
    db_pool: PgPool,
    system_token: Arc<RwLock<Option<SystemToken>>>,
}

#[derive(Debug, Clone)]
struct SystemToken {
    token: PlaintextToken,
    created_at: std::time::Instant,
}

#[derive(Debug, thiserror::Error)]
pub enum TokenManagerError {
    DatabaseError(sqlx::Error),
    InvalidInput(String),
    TokenLimitReached,
    TokenNotFound,
    FailedToHashToken(String),
    InvalidCredentials,
}

impl std::fmt::Display for TokenManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenManagerError::DatabaseError(e) => write!(f, "Database error: {}", e),
            TokenManagerError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            TokenManagerError::TokenLimitReached => write!(f, "Token limit reached"),
            TokenManagerError::TokenNotFound => write!(f, "Token not found"),
            TokenManagerError::FailedToHashToken(msg) => write!(f, "Failed to hash token: {}", msg),
            TokenManagerError::InvalidCredentials => write!(f, "Invalid credentials"),
        }
    }
}

const MAX_TOKENS_PER_USER: i64 = 10;
const BCRYPT_COST: u32 = 12;

pub const SYSTEM_USERNAME: &str = "system";
const SYSTEM_TOKEN_LIFETIME_SECS: u64 = 15 * 60; // 15 minutes

impl TokenManager {
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            db_pool,
            system_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new registry token for a user
    /// Returns the token ID and the plaintext token (only time it's visible)
    pub async fn create_token(
        &self,
        user_id: &UserId,
        name: &TokenName,
    ) -> Result<(UserId, PlaintextToken), TokenManagerError> {
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
        let token_id = sqlx::query!(
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

        Ok((token_id, plaintext_token))
    }

    /// Get or create a system token for this website instance. This token is
    /// cached in memory and reused across requests. Returns the plaintext token
    /// that can be used for registry authentication
    pub async fn get_system_token(&self) -> Result<PlaintextToken, TokenManagerError> {
        // Check if we have a valid cached token with enough time remaining
        {
            let guard = self.system_token.read().await;
            if let Some(sys_token) = guard.as_ref() {
                // Check database to see if token has at least 5 minutes remaining
                let has_time_remaining = sqlx::query!(
                    r#"
                    SELECT EXISTS(
                        SELECT 1 FROM registry_tokens_internal
                        WHERE token_hash = $1 AND expires_at > now() + interval '5 minutes'
                    ) as "exists!"
                    "#,
                    bcrypt::hash(sys_token.token.as_ref(), BCRYPT_COST)
                        .map_err(|e| TokenManagerError::FailedToHashToken(e.to_string()))?
                )
                .fetch_one(&self.db_pool)
                .await
                .map_err(TokenManagerError::DatabaseError)?
                .exists;

                if has_time_remaining {
                    tracing::debug!("Reusing cached system token");
                    return Ok(sys_token.token.clone());
                }

                tracing::debug!("Cached token expiring soon, generating new one");
            }
        }

        // Generate new token
        tracing::debug!("Creating new system token");
        let plaintext_token = PlaintextToken::generate();
        let token_hash = bcrypt::hash(plaintext_token.as_ref(), BCRYPT_COST)
            .map_err(|e| TokenManagerError::FailedToHashToken(e.to_string()))?;

        // Store hash in database
        sqlx::query!(
            r#"
            INSERT INTO registry_tokens_internal (token_hash)
            VALUES ($1)
            "#,
            token_hash
        )
        .execute(&self.db_pool)
        .await
        .map_err(TokenManagerError::DatabaseError)?;

        // Cache the plaintext token
        let mut guard = self.system_token.write().await;
        *guard = Some(SystemToken {
            token: plaintext_token.clone(),
            created_at: std::time::Instant::now(),
        });

        Ok(plaintext_token)
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
        token: &str,
    ) -> Result<(), TokenManagerError> {
        for db_token in self.get_active_tokens(user_id).await? {
            if bcrypt::verify(token, &db_token.token_hash).unwrap_or(false) {
                return Ok(());
            }
        }

        Err(TokenManagerError::InvalidCredentials)
    }

    pub(crate) async fn validate_system_token(&self, token: &str) -> Result<(), TokenManagerError> {
        todo!()
    }
}
