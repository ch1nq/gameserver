use std::ops::Deref;
use std::str::FromStr;

#[derive(Debug, Clone, sqlx::Type, serde::Deserialize, serde::Serialize)]
#[sqlx(type_name = "agent_status", rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Inactive,
}

/// Agent name (3-50 alphanumeric/hyphen/underscore chars)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentName(String);

impl AgentName {
    pub fn new(s: &str) -> Result<Self, String> {
        Self::from_str(s)
    }
}

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

impl Deref for AgentName {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for AgentName {
    fn from(s: String) -> Self {
        Self::from_str(&s).expect("Invalid agent name")
    }
}
