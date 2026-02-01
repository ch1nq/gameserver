//! Registry client for listing user images.

use serde::Deserialize;

/// Client for interacting with the Docker registry.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    http_client: reqwest::Client,
    registry_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogResponse {
    repositories: Vec<String>,
}

impl RegistryClient {
    pub fn new(registry_url: String) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            registry_url,
        }
    }

    /// List images for a user namespace.
    ///
    /// Returns a list of image names (without the namespace prefix).
    pub async fn list_user_images(
        &self,
        user_id: i64,
        token: &str,
    ) -> Result<Vec<String>, RegistryError> {
        let namespace = format!("user-{}/", user_id);

        // Fetch catalog from registry
        let catalog_url = format!("{}/v2/_catalog", self.registry_url);
        let response = self
            .http_client
            .get(&catalog_url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| {
                tracing::info!("{}", e.to_string());
                RegistryError::Connection(e.to_string())
            })?;

        if !response.status().is_success() {
            return Err(RegistryError::Api(format!(
                "Registry returned error: {}",
                response.status()
            )));
        }

        let catalog: CatalogResponse = response
            .json()
            .await
            .map_err(|e| RegistryError::Parse(e.to_string()))?;

        // Filter repositories for this user's namespace and strip the prefix
        let images: Vec<String> = catalog
            .repositories
            .into_iter()
            .filter(|repo| repo.starts_with(&namespace))
            .map(|repo| repo.strip_prefix(&namespace).unwrap_or(&repo).to_string())
            .collect();

        Ok(images)
    }
}

#[derive(Debug, Clone)]
pub enum RegistryError {
    Connection(String),
    Api(String),
    Parse(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Connection(e) => write!(f, "Failed to connect to registry: {}", e),
            RegistryError::Api(e) => write!(f, "Registry API error: {}", e),
            RegistryError::Parse(e) => write!(f, "Failed to parse registry response: {}", e),
        }
    }
}

impl std::error::Error for RegistryError {}
