//! Token types and utilities for Docker registry authentication

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A validated token name (3-50 characters, alphanumeric + spaces/hyphens/underscores)
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

/// Hash of a registry token (bcrypt)
pub type TokenHash = String;

/// A plaintext token (only visible during creation)
#[derive(Debug, Clone)]
pub struct PlaintextToken(String);

impl PlaintextToken {
    /// Generate a random token of 64 alphanumeric characters
    pub fn generate() -> Self {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::rng();
        let chars = (0..64)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        Self(chars)
    }

    /// Hash this token using bcrypt
    pub fn hash(&self, cost: u32) -> Result<TokenHash, bcrypt::BcryptError> {
        bcrypt::hash(&self.0, cost)
    }
}

impl AsRef<str> for PlaintextToken {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<PlaintextToken> for String {
    fn from(token: PlaintextToken) -> String {
        token.0
    }
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
        let token = PlaintextToken::generate();
        assert_eq!(token.0.len(), 64);
        assert!(token.0.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_token_hash() {
        let token = PlaintextToken::generate();
        let hash = token.hash(4).expect("hashing should succeed");
        assert!(bcrypt::verify(token.as_ref(), &hash).unwrap());
    }
}
