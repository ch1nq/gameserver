//! Docker Registry v2 Token Authentication Library
//!
//! This library provides Docker Registry v2 token authentication following the spec:
//! <https://docs.docker.com/reference/api/registry/auth/>
//!
//! ## Features
//!
//! - JWT token generation for Docker Registry authentication
//! - Token storage abstraction via `TokenStorage` trait
//! - Axum router integration (optional, via `axum-integration` feature)
//! - User namespace validation
//! - System token support for internal services

pub mod auth;
pub mod storage;
pub mod token;

// Re-exports for convenience
pub use auth::{RegistryAuthConfig, RegistryJwtToken};
pub use token::{PlaintextToken, TokenName};

#[cfg(feature = "axum-integration")]
pub use auth::router;
