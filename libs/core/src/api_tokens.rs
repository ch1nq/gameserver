use crate::users::UserId;
pub use common::ApiTokenId;
use registry_auth::{PlaintextToken, TokenName};
use sqlx::PgPool;

/// API token record from database
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiToken {
    pub id: ApiTokenId,
    pub user_id: UserId,
    pub name: String,
    #[serde(skip)]
    pub token_hash: String,
    pub created_at: time::PrimitiveDateTime,
    pub revoked_at: Option<time::PrimitiveDateTime>,
}

impl From<ApiToken> for api_types::ApiToken {
    fn from(t: ApiToken) -> Self {
        Self {
            id: t.id,
            user_id: t.user_id,
            name: t.name,
            created_at: t.created_at,
            revoked_at: t.revoked_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiTokenManager {
    db_pool: PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiTokenError {
    #[error("Database error: {0}")]
    DatabaseError(sqlx::Error),

    #[error("Token limit reached")]
    TokenLimitReached,

    #[error("Token not found")]
    TokenNotFound,

    #[error("Failed to hash token: {0}")]
    FailedToHashToken(String),

    #[error("Invalid credentials")]
    InvalidCredentials,
}

const MAX_TOKENS_PER_USER: i64 = 10;
const BCRYPT_COST: u32 = 12;

impl ApiTokenManager {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Create a new API token for a user.
    /// Returns the plaintext token (only visible at creation time).
    pub async fn create_token(
        &self,
        user_id: &UserId,
        name: &TokenName,
    ) -> Result<PlaintextToken, ApiTokenError> {
        let count = self.count_active_tokens(user_id).await?;
        if count >= MAX_TOKENS_PER_USER {
            return Err(ApiTokenError::TokenLimitReached);
        }

        let plaintext_token = PlaintextToken::generate();

        let token_hash = bcrypt::hash(plaintext_token.as_ref(), BCRYPT_COST)
            .map_err(|e| ApiTokenError::FailedToHashToken(e.to_string()))?;

        sqlx::query!(
            r#"
            INSERT INTO api_tokens (user_id, token_hash, name)
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
            user_id,
            token_hash,
            name.as_ref(),
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(ApiTokenError::DatabaseError)?;

        Ok(plaintext_token)
    }

    /// List all active (non-revoked) API tokens for a user.
    pub async fn list_tokens(&self, user_id: &UserId) -> Result<Vec<ApiToken>, ApiTokenError> {
        sqlx::query_as!(
            ApiToken,
            r#"
            SELECT id, user_id, name, token_hash, created_at, revoked_at
            FROM api_tokens
            WHERE user_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(ApiTokenError::DatabaseError)
    }

    /// Revoke an API token (soft delete).
    pub async fn revoke_token(
        &self,
        user_id: &UserId,
        token_id: ApiTokenId,
    ) -> Result<(), ApiTokenError> {
        let result = sqlx::query!(
            r#"
            UPDATE api_tokens
            SET revoked_at = NOW()
            WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL
            "#,
            token_id,
            user_id,
        )
        .execute(&self.db_pool)
        .await
        .map_err(ApiTokenError::DatabaseError)?;

        if result.rows_affected() == 0 {
            return Err(ApiTokenError::TokenNotFound);
        }

        Ok(())
    }

    /// Validate an API token for a user.
    pub async fn validate_token(
        &self,
        user_id: &UserId,
        token_plaintext: &str,
    ) -> Result<(), ApiTokenError> {
        let candidates = self.list_tokens(user_id).await?;
        for candidate in candidates {
            if bcrypt::verify(token_plaintext, &candidate.token_hash).unwrap_or(false) {
                return Ok(());
            }
        }
        Err(ApiTokenError::InvalidCredentials)
    }

    /// Count active tokens for a user.
    async fn count_active_tokens(&self, user_id: &UserId) -> Result<i64, ApiTokenError> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as "count!"
            FROM api_tokens
            WHERE user_id = $1 AND revoked_at IS NULL
            "#,
            user_id
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(ApiTokenError::DatabaseError)?
        .count;

        Ok(count)
    }
}
