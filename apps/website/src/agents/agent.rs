use sqlx::FromRow;
use std::str::FromStr;

#[derive(Debug, Clone, sqlx::Type, serde::Deserialize, serde::Serialize)]
#[sqlx(type_name = "agent_status", rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Inactive,
}

pub type AgentId = i64;

/// Container image URL (e.g., "ghcr.io/user/agent:latest", "http://localhost:5000/user-1234/agent:v1")
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageUrl(String);

impl ImageUrl {
    /// Validate and create a new ImageUrl from user input
    pub fn new(s: String) -> Result<Self, String> {
        if s.trim().is_empty() {
            return Err("Image URL cannot be empty".to_string());
        }
        Ok(Self(s))
    }

    pub fn repository(&self) -> String {
        self.0
            .split_once(':')
            .map(|(repo, _)| repo)
            .unwrap_or(&self.0)
            .to_string()
    }
}

// For SQLx deserialization from database. Use ImageUrl::new for user input validation.
impl From<String> for ImageUrl {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl ToString for ImageUrl {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}

impl AsRef<str> for ImageUrl {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Agent name (3-50 alphanumeric/hyphen/underscore chars)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentName(String);

impl FromStr for AgentName {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 3 || s.len() > 50 {
            return Err("Name must be 3-50 characters".to_string());
        }
        if !s
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err("Name can only contain alphanumeric, hyphens, and underscores".to_string());
        }
        Ok(Self(s.to_string()))
    }
}

impl From<String> for AgentName {
    fn from(s: String) -> Self {
        Self::from_str(&s).expect("Invalid agent name")
    }
}

impl AsRef<str> for AgentName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Into<String> for AgentName {
    fn into(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Agent {
    pub id: AgentId,
    pub name: AgentName,
    pub user_id: crate::users::UserId,
    pub status: AgentStatus,
    pub image_url: ImageUrl,
}
