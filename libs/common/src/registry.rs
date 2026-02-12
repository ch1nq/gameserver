use crate::UserId;
use std::ops::Deref;

/// Errors that can occur when parsing image URLs
#[derive(Debug, Clone, thiserror::Error)]
pub enum ImageParseError {
    #[error("Image URL cannot be empty")]
    Empty,

    #[error("Invalid format: expected 'repository:tag' or 'user-{{id}}/repository:tag'")]
    InvalidFormat,

    #[error("Invalid repository name: {0}")]
    InvalidRepository(String),

    #[error("Invalid tag: {0}")]
    InvalidTag(String),

    #[error("Image does not belong to user {expected}, found namespace for user {found}")]
    NamespaceMismatch { expected: UserId, found: UserId },

    #[error("Could not extract user ID from namespace")]
    MissingNamespace,
}

/// Common interface for any container image reference
pub trait ContainerImageUrl: AsRef<str> + std::fmt::Display {
    /// Get the full image URL as a string
    fn as_url(&self) -> &str;

    /// Get repository without tag (e.g., "user-123/my-bot")
    fn repository(&self) -> String;

    /// Get the tag (e.g., "v1", "latest")
    fn tag(&self) -> &str;

    /// Convert to a generic ImageUrl for infrastructure use
    fn to_image_url(&self) -> ImageUrl;
}

/// User agent image in local registry (validated, namespace-scoped)
///
/// Format: `user-{user_id}/{repository}:{tag}`
///
/// Examples:
/// - "user-123/my-bot:v1"
/// - "user-456/test-agent:latest"
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AgentImageUrl {
    user_id: UserId,
    repository: String,
    tag: String,
    // Cached full URL for efficiency
    #[serde(skip)]
    full_url: String,
}

impl AgentImageUrl {
    /// Parse from short format: "my-bot:v1"
    ///
    /// Constructs full URL as "user-{user_id}/my-bot:v1"
    /// If no tag is specified, defaults to "latest"
    pub fn parse(user_id: UserId, image: &str) -> Result<Self, ImageParseError> {
        if image.trim().is_empty() {
            return Err(ImageParseError::Empty);
        }

        // Split on colon to separate repository and tag
        let (repository, tag) = match image.split_once(':') {
            Some((repo, tag)) => (repo, tag),
            None => (image, "latest"),
        };

        // Validate repository name
        if repository.is_empty() || repository.len() > 100 {
            return Err(ImageParseError::InvalidRepository(
                "Repository name must be 1-100 characters".to_string(),
            ));
        }

        if !repository
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/')
        {
            return Err(ImageParseError::InvalidRepository(
                "Repository name can only contain alphanumeric, hyphens, underscores, and slashes"
                    .to_string(),
            ));
        }

        // Validate tag
        if tag.is_empty() || tag.len() > 128 {
            return Err(ImageParseError::InvalidTag(
                "Tag must be 1-128 characters".to_string(),
            ));
        }

        if !tag
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(ImageParseError::InvalidTag(
                "Tag can only contain alphanumeric, hyphens, underscores, and dots".to_string(),
            ));
        }

        let full_url = format!("user-{}/{}:{}", user_id, repository, tag);

        Ok(Self {
            user_id,
            repository: repository.to_string(),
            tag: tag.to_string(),
            full_url,
        })
    }

    /// Parse from full format: "user-123/my-bot:v1"
    ///
    /// Validates that namespace matches expected_user_id
    /// If no tag is specified, defaults to "latest"
    pub fn parse_full(image: &str, expected_user_id: UserId) -> Result<Self, ImageParseError> {
        if image.trim().is_empty() {
            return Err(ImageParseError::Empty);
        }

        // Extract namespace (user-{id}/)
        let namespace_prefix = "user-";
        if !image.starts_with(namespace_prefix) {
            return Err(ImageParseError::InvalidFormat);
        }

        // Find the slash after user-{id}
        let after_prefix = &image[namespace_prefix.len()..];
        let slash_pos = after_prefix
            .find('/')
            .ok_or(ImageParseError::InvalidFormat)?;

        // Extract user_id
        let user_id_str = &after_prefix[..slash_pos];
        let user_id: UserId = user_id_str
            .parse()
            .map_err(|_| ImageParseError::InvalidFormat)?;

        // Check namespace matches
        if user_id != expected_user_id {
            return Err(ImageParseError::NamespaceMismatch {
                expected: expected_user_id,
                found: user_id,
            });
        }

        // Parse the rest (repository:tag)
        let rest = &after_prefix[slash_pos + 1..];
        Self::parse(user_id, rest)
    }

    /// Get the user ID this image belongs to
    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    /// Get repository name without namespace or tag (e.g., "my-bot")
    pub fn repository_name(&self) -> &str {
        &self.repository
    }

    /// Get the tag (e.g., "v1")
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Check if this image belongs to a specific user
    pub fn belongs_to_user(&self, user_id: UserId) -> bool {
        self.user_id == user_id
    }

    /// Get the full repository path with namespace (e.g., "user-123/my-bot")
    pub fn repository_with_namespace(&self) -> String {
        format!("user-{}/{}", self.user_id, self.repository)
    }
}

impl ContainerImageUrl for AgentImageUrl {
    fn as_url(&self) -> &str {
        &self.full_url
    }

    fn repository(&self) -> String {
        self.repository_with_namespace()
    }

    fn tag(&self) -> &str {
        &self.tag
    }

    fn to_image_url(&self) -> ImageUrl {
        ImageUrl::from(self.full_url.clone())
    }
}

impl AsRef<str> for AgentImageUrl {
    fn as_ref(&self) -> &str {
        &self.full_url
    }
}

impl std::fmt::Display for AgentImageUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_url)
    }
}

impl From<AgentImageUrl> for String {
    fn from(img: AgentImageUrl) -> Self {
        img.full_url
    }
}

impl TryFrom<String> for AgentImageUrl {
    type Error = ImageParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        // Parse the namespace to extract user_id
        let namespace_prefix = "user-";
        if !s.starts_with(namespace_prefix) {
            return Err(ImageParseError::MissingNamespace);
        }

        let after_prefix = &s[namespace_prefix.len()..];
        let slash_pos = after_prefix
            .find('/')
            .ok_or(ImageParseError::MissingNamespace)?;

        let user_id_str = &after_prefix[..slash_pos];
        let user_id: UserId = user_id_str
            .parse()
            .map_err(|_| ImageParseError::InvalidFormat)?;

        Self::parse_full(&s, user_id)
    }
}

/// Generic container image URL (game host, external images, etc.)
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
}

impl ContainerImageUrl for ImageUrl {
    fn as_url(&self) -> &str {
        &self.0
    }

    fn repository(&self) -> String {
        self.split_once(':')
            .map(|(repo, _)| repo.to_string())
            .unwrap_or_else(|| self.0.clone())
    }

    fn tag(&self) -> &str {
        self.split_once(':').map(|(_, tag)| tag).unwrap_or("latest")
    }

    fn to_image_url(&self) -> ImageUrl {
        self.clone()
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

impl std::fmt::Display for ImageUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_image_parse_short_format() {
        let img = AgentImageUrl::parse(123, "my-bot:v1").unwrap();
        assert_eq!(img.user_id(), 123);
        assert_eq!(img.repository_name(), "my-bot");
        assert_eq!(img.tag(), "v1");
        assert_eq!(img.as_url(), "user-123/my-bot:v1");
    }

    #[test]
    fn test_agent_image_parse_short_format_no_tag() {
        let img = AgentImageUrl::parse(123, "my-bot").unwrap();
        assert_eq!(img.user_id(), 123);
        assert_eq!(img.repository_name(), "my-bot");
        assert_eq!(img.tag(), "latest");
        assert_eq!(img.as_url(), "user-123/my-bot:latest");
    }

    #[test]
    fn test_agent_image_parse_full_format() {
        let img = AgentImageUrl::parse_full("user-123/my-bot:v1", 123).unwrap();
        assert_eq!(img.user_id(), 123);
        assert_eq!(img.repository_name(), "my-bot");
        assert_eq!(img.tag(), "v1");
        assert_eq!(img.as_url(), "user-123/my-bot:v1");
    }

    #[test]
    fn test_agent_image_parse_full_format_no_tag() {
        let img = AgentImageUrl::parse_full("user-123/my-bot", 123).unwrap();
        assert_eq!(img.user_id(), 123);
        assert_eq!(img.repository_name(), "my-bot");
        assert_eq!(img.tag(), "latest");
        assert_eq!(img.as_url(), "user-123/my-bot:latest");
    }

    #[test]
    fn test_agent_image_parse_full_format_namespace_mismatch() {
        let result = AgentImageUrl::parse_full("user-123/my-bot:v1", 456);
        assert!(matches!(
            result,
            Err(ImageParseError::NamespaceMismatch {
                expected: 456,
                found: 123
            })
        ));
    }

    #[test]
    fn test_agent_image_parse_invalid_empty() {
        let result = AgentImageUrl::parse(123, "");
        assert!(matches!(result, Err(ImageParseError::Empty)));
    }

    #[test]
    fn test_agent_image_parse_invalid_format() {
        let result = AgentImageUrl::parse_full("invalid-format", 123);
        assert!(matches!(result, Err(ImageParseError::InvalidFormat)));
    }

    #[test]
    fn test_agent_image_repository_name() {
        let img = AgentImageUrl::parse(123, "my-bot:v1").unwrap();
        assert_eq!(img.repository_name(), "my-bot");
    }

    #[test]
    fn test_agent_image_repository_with_namespace() {
        let img = AgentImageUrl::parse(123, "my-bot:v1").unwrap();
        assert_eq!(img.repository_with_namespace(), "user-123/my-bot");
    }

    #[test]
    fn test_agent_image_belongs_to_user() {
        let img = AgentImageUrl::parse(123, "my-bot:v1").unwrap();
        assert!(img.belongs_to_user(123));
        assert!(!img.belongs_to_user(456));
    }

    #[test]
    fn test_agent_image_trait_methods() {
        let img = AgentImageUrl::parse(123, "my-bot:v1").unwrap();
        assert_eq!(img.as_url(), "user-123/my-bot:v1");
        assert_eq!(img.repository(), "user-123/my-bot");
        assert_eq!(img.tag(), "v1");
    }

    #[test]
    fn test_agent_image_to_image_url() {
        let img = AgentImageUrl::parse(123, "my-bot:v1").unwrap();
        let generic = img.to_image_url();
        assert_eq!(generic.as_ref(), "user-123/my-bot:v1");
    }

    #[test]
    fn test_agent_image_serde_roundtrip() {
        let img = AgentImageUrl::parse(123, "my-bot:v1").unwrap();
        let json = serde_json::to_string(&img).unwrap();
        assert_eq!(json, "\"user-123/my-bot:v1\"");

        let deserialized: AgentImageUrl = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, img);
    }

    #[test]
    fn test_agent_image_string_conversions() {
        let img = AgentImageUrl::parse(123, "my-bot:v1").unwrap();
        let s: String = img.clone().into();
        assert_eq!(s, "user-123/my-bot:v1");

        let img2: AgentImageUrl = s.try_into().unwrap();
        assert_eq!(img2, img);
    }

    #[test]
    fn test_agent_image_display() {
        let img = AgentImageUrl::parse(123, "my-bot:v1").unwrap();
        assert_eq!(format!("{}", img), "user-123/my-bot:v1");
    }

    #[test]
    fn test_image_url_trait_methods() {
        let img = ImageUrl::from("ghcr.io/user/repo:v1".to_string());
        assert_eq!(img.as_url(), "ghcr.io/user/repo:v1");
        assert_eq!(img.repository(), "ghcr.io/user/repo");
        assert_eq!(img.tag(), "v1");
    }

    #[test]
    fn test_image_url_no_tag() {
        let img = ImageUrl::from("nginx".to_string());
        assert_eq!(img.repository(), "nginx");
        assert_eq!(img.tag(), "latest");
    }

    #[test]
    fn test_image_url_display() {
        let img = ImageUrl::from("nginx:latest".to_string());
        assert_eq!(format!("{}", img), "nginx:latest");
    }
}
