use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::users::UserId;

/// A validated token name (3-50 characters, alphanumeric + spaces/hyphens)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenName(String);

impl TokenName {
    pub fn new(name: String) -> Result<Self, String> {
        let trimmed = name.trim();

        if trimmed.len() < 3 {
            return Err("Token name must be at least 3 characters long".to_string());
        }

        if trimmed.len() > 50 {
            return Err("Token name must not exceed 50 characters".to_string());
        }

        if !trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
        {
            return Err("Token name can only contain alphanumeric characters, spaces, hyphens, and underscores".to_string());
        }

        Ok(Self(trimmed.to_string()))
    }
}

impl FromStr for TokenName {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.to_string())
    }
}

impl AsRef<str> for TokenName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TokenName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

type RegistryTokenHash = String;

/// Registry token record from database
#[derive(Debug, Clone)]
pub struct RegistryToken {
    pub id: i64,
    pub user_id: UserId,
    pub name: String,
    pub token_hash: RegistryTokenHash,
    pub created_at: time::PrimitiveDateTime,
    pub revoked_at: Option<time::PrimitiveDateTime>,
}

/// Generate a secure random token (64 alphanumeric characters)
pub fn generate_token() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();

    (0..64)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_name_validation() {
        assert!(TokenName::new("Valid Token".to_string()).is_ok());
        assert!(TokenName::new("CI-Token-123".to_string()).is_ok());
        assert!(TokenName::new("ab".to_string()).is_err()); // Too short
        assert!(TokenName::new("a".repeat(51)).is_err()); // Too long
        assert!(TokenName::new("Invalid@Token".to_string()).is_err()); // Invalid char
    }

    #[test]
    fn test_generate_token() {
        let token = generate_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_alphanumeric()));
    }
}
