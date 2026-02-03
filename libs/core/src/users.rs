use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

pub use common::UserId;

#[derive(Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub access_token: String,
}

// Here we've implemented `Debug` manually to avoid accidentally logging the
// access token.
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("access_token", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct UserManager {
    db_pool: PgPool,
}

impl UserManager {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Look up a user by ID.
    pub async fn get_user(&self, user_id: UserId) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.db_pool)
            .await
    }
}

#[cfg(feature = "axum-login")]
impl axum_login::AuthUser for User {
    type Id = UserId;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.access_token.as_bytes()
    }
}
