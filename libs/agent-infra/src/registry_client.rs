use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct RegistryClient {
    http_client: reqwest::Client,
    registry_url: String,
}

type Namespace = String;
type ImageUrl = String;
type RegistryToken = String;
type Error = String;

#[derive(Debug, Clone, Deserialize)]
struct CatalogResponse {
    repositories: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BasicRegistryCredentials {
    pub username: String,
    pub password: String,
}

impl RegistryClient {
    pub fn new(registry_url: String, http_client: reqwest::Client) -> Self {
        Self {
            http_client,
            registry_url,
        }
    }

    pub async fn list_images(
        &self,
        namespace: &Namespace,
        token: &RegistryToken,
    ) -> Result<Vec<ImageUrl>, Error> {
        // Fetch catalog from registry
        let catalog_url = format!("{}/v2/_catalog", self.registry_url);
        let response = self
            .http_client
            .get(&catalog_url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to registry: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Registry returned error: {}", response.status()));
        }

        let catalog: CatalogResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse registry response: {}", e))?;

        // Filter repositories for this user's namespace: "{namespace}/*"
        let namespace = with_slash(namespace);
        let images: Vec<ImageUrl> = catalog
            .repositories
            .into_iter()
            .filter(|repo| repo.starts_with(namespace.as_str()))
            .collect();

        Ok(images)
    }

    pub async fn copy_image(
        &self,
        source_image_url: &ImageUrl,
        destination_image_url: &ImageUrl,
        source_token: &String,
        destination_credentials: &BasicRegistryCredentials,
    ) -> Result<(), Error> {
        let status = tokio::process::Command::new("skopeo")
            .arg("copy")
            .arg(format!("docker://{}", source_image_url))
            .arg(format!("docker://{}", destination_image_url))
            .arg("--src-tls-verify=false")
            .arg("--src-registry-token")
            .arg(source_token)
            .arg("--dest-creds")
            .arg(format!(
                "{}:{}",
                destination_credentials.username, destination_credentials.password
            ))
            .status()
            .await
            .map_err(|e| format!("Failed to execute skopeo: {}", e))?;
        if !status.success() {
            return Err(format!("Skopeo failed with status: {}", status));
        }
        Ok(())
    }
}

/// Ensure the string ends with a slash
fn with_slash(s: &str) -> String {
    if s.ends_with('/') {
        s.to_string()
    } else {
        format!("{}/", s)
    }
}
