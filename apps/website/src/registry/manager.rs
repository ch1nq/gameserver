use super::token::{RegistryToken, TokenName, generate_token};
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct TokenManager {
    db_pool: PgPool,
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

impl TokenManager {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Create a new registry token for a user
    /// Returns the token ID and the plaintext token (only time it's visible)
    pub async fn create_token(
        &self,
        user_id: &i64,
        name: &TokenName,
    ) -> Result<(i64, String), TokenManagerError> {
        // Check token limit
        let count = self.count_active_tokens(user_id).await?;
        if count >= MAX_TOKENS_PER_USER {
            return Err(TokenManagerError::TokenLimitReached);
        }

        // Generate plaintext token
        let plaintext_token = generate_token();

        // Hash the token using bcrypt
        let token_hash = bcrypt::hash(&plaintext_token, BCRYPT_COST)
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

    /// List all active (non-revoked) tokens for a user
    pub async fn list_tokens(
        &self,
        user_id: &i64,
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
        user_id: &i64,
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
    pub async fn count_active_tokens(&self, user_id: &i64) -> Result<i64, TokenManagerError> {
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
        user_id: &i64,
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
        user_id: &i64,
        token: &str,
    ) -> Result<(), TokenManagerError> {
        for db_token in self.get_active_tokens(user_id).await? {
            if bcrypt::verify(token, &db_token.token_hash).unwrap_or(false) {
                return Ok(());
            }
        }

        Err(TokenManagerError::InvalidCredentials)
    }
}
