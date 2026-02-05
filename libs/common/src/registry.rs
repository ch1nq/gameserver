use std::ops::Deref;

/// Container image URL (e.g., "ghcr.io/user/agent:latest" or "user/agent:v1")
///
/// Can represent:
/// - Fully qualified public registry URLs: "ghcr.io/org/repo:tag"
/// - Docker Hub URLs: "docker.io/user/repo:tag" or "user/repo:tag"
/// - Private registry URLs: "registry.example.com/repo:tag"
/// - Local registry images: "user-123/agent:v1"
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageUrl(String);

impl ImageUrl {
    /// Create a new ImageUrl with validation
    pub fn new(s: String) -> Result<Self, String> {
        if s.trim().is_empty() {
            return Err("Image URL cannot be empty".to_string());
        }
        Ok(Self(s))
    }

    /// Extract the repository part (without tag)
    ///
    /// Examples:
    /// - "ghcr.io/user/agent:latest" -> "ghcr.io/user/agent"
    /// - "user/agent:v1" -> "user/agent"
    pub fn repository(&self) -> String {
        self.split_once(':')
            .map(|(repo, _)| repo)
            .unwrap_or(self)
            .to_string()
    }
}

impl Deref for ImageUrl {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for ImageUrl {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for ImageUrl {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Registry authentication token for pulling private images
///
/// Represents a JWT token or other credential used to authenticate
/// with a container registry (Docker registry protocol).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegistryToken(String);

impl RegistryToken {
    /// Create a new registry token
    pub fn new(token: String) -> Self {
        Self(token)
    }
}

impl Deref for RegistryToken {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for RegistryToken {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for RegistryToken {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
