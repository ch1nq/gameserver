pub mod client;
pub mod manager;
pub mod token;

pub use client::RegistryClient;
pub use manager::{RegistryPrincipal, RegistryTokenManager};
pub use token::{RegistryToken, TokenName};

// Re-export from registry-auth library
pub use registry_auth::RegistryAuthConfig;
