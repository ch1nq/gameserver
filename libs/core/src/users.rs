use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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
