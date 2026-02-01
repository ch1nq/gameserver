use crate::users::UserId;

// Re-export token types from registry-auth
pub use registry_auth::{PlaintextToken, TokenName};

pub type RegistryTokenHash = String;

type RegistryTokenId = i64;

/// Registry token record from database
#[derive(Debug, Clone)]
pub struct RegistryToken {
    pub id: RegistryTokenId,
    pub user_id: UserId,
    pub name: String,
    pub token_hash: RegistryTokenHash,
    pub created_at: time::PrimitiveDateTime,
    pub revoked_at: Option<time::PrimitiveDateTime>,
}
