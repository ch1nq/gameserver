use super::token::{RegistryToken, TokenName, generate_token};
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct TokenManager {
    db_pool: PgPool,
}

type TokenManagerError = Box<dyn std::error::Error>;

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
        user_id: i64,
        name: TokenName,
    ) -> Result<(i64, String), TokenManagerError> {
        // Check token limit
        let count = self.count_active_tokens(user_id).await?;
        if count >= MAX_TOKENS_PER_USER {
            return Err(format!(
                "Maximum of {} active tokens reached. Please revoke an existing token first.",
                MAX_TOKENS_PER_USER
            )
            .into());
        }

        // Generate plaintext token
        let plaintext_token = generate_token();

        // Hash the token using bcrypt
        let token_hash = bcrypt::hash(&plaintext_token, BCRYPT_COST)
            .map_err(|e| format!("Failed to hash token: {}", e))?;

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
        .await?
        .id;

        Ok((token_id, plaintext_token))
    }

    /// List all active (non-revoked) tokens for a user
    pub async fn list_tokens(&self, user_id: i64) -> Result<Vec<RegistryToken>, TokenManagerError> {
        let tokens = sqlx::query_as!(
            RegistryToken,
            r#"
            SELECT id, user_id, name, created_at, revoked_at
            FROM registry_tokens
            WHERE user_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.db_pool)
        .await?;

        Ok(tokens)
    }

    /// Revoke a token (soft delete by setting revoked_at)
    pub async fn revoke_token(&self, user_id: i64, token_id: i64) -> Result<(), TokenManagerError> {
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
        .await?;

        if result.rows_affected() == 0 {
            return Err("Token not found or already revoked".into());
        }

        Ok(())
    }

    /// Count active tokens for a user
    async fn count_active_tokens(&self, user_id: i64) -> Result<i64, TokenManagerError> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as "count!"
            FROM registry_tokens
            WHERE user_id = $1 AND revoked_at IS NULL
            "#,
            user_id
        )
        .fetch_one(&self.db_pool)
        .await?
        .count;

        Ok(count)
    }
}
