//! Registry client for listing user images.

use common::AgentImageUrl;
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
    /// Returns a list of validated AgentImageUrl instances.
    pub async fn list_user_images(
        &self,
        user_id: common::UserId,
        token: &str,
    ) -> Result<Vec<AgentImageUrl>, RegistryError> {
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

        // Filter repositories for this user's namespace, strip prefix, and parse to AgentImageUrl
        let images: Vec<AgentImageUrl> = catalog
            .repositories
            .into_iter()
            .filter(|repo| repo.starts_with(&namespace))
            .filter_map(|repo| {
                let image_name = repo.strip_prefix(&namespace).unwrap_or(&repo);
                // Parse to AgentImageUrl - registry images may not have tags, so we default to :latest
                match AgentImageUrl::parse(user_id, image_name) {
                    Ok(img) => Some(img),
                    Err(e) => {
                        tracing::warn!(
                            user_id = user_id,
                            image = image_name,
                            error = %e,
                            "Failed to parse image from registry"
                        );
                        None
                    }
                }
            })
            .collect();

        Ok(images)
    }

    /// Check if a specific image exists in the user's namespace.
    ///
    /// Validates repository name only (ignores tag) since registry catalog
    /// doesn't include tag information.
    pub async fn image_exists(
        &self,
        user_id: common::UserId,
        image: &AgentImageUrl,
        token: &str,
    ) -> Result<bool, RegistryError> {
        let available_images = self.list_user_images(user_id, token).await?;

        // Compare repository name (without tag)
        let image_repo = image.repository_name();

        Ok(available_images
            .iter()
            .any(|img| img.repository_name() == image_repo))
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
