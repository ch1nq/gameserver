use nonzero_ext::nonzero;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};

type FlyAppName = String;
type FlyNetwork = String;
type FlyOrg = String;
type FlyServiceName = String;
type FlyEnv = HashMap<String, String>;
type ImageUrl = String;

/// https://docs.machines.dev/#tag/apps/post/apps
#[derive(Debug, Serialize, Deserialize)]
struct CreateAppRequest {
    name: FlyAppName,
    org_slug: FlyOrg,
    network: FlyNetwork,
}

/// https://docs.machines.dev/#tag/apps/post/apps/{app_name}/ip_assignments
#[derive(Debug, Serialize, Deserialize)]
struct AssignIpRequest {
    network: FlyNetwork,
    org_slug: FlyOrg,
    service_name: FlyServiceName,
    #[serde(rename = "type")]
    ip_type: FlyIpType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FlyIpType {
    PrivateV6,
}

/// https://docs.machines.dev/#tag/machines/post/apps/{app_name}/machines
#[derive(Debug, Serialize, Deserialize)]
struct CreateMachineRequest {
    config: FlyMachineConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct FlyMachineConfig {
    pub image: ImageUrl,
    pub env: FlyEnv,
    pub auto_destroy: bool,
    pub restart: FlyRestartConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct FlyRestartConfig {
    pub max_retries: u32,
    pub policy: FlyRestartPolicy,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FlyRestartPolicy {
    /// Never try to restart a Machine automatically.
    No,
    /// Always restart a Machine automatically.
    Always,
    /// Try up to MaxRetries times to restart on non-zero exit.
    OnFailure,
}

/// Response from creating a machine
/// https://docs.machines.dev/#tag/machines/post/apps/{app_name}/machines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CreateMachineResponse {
    pub id: String,
    pub private_ip: String,
}

/// Response from listing apps in an organization
/// https://docs.machines.dev/#tag/apps/get/apps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ListAppsResponse {
    pub total_apps: usize,
    pub apps: Vec<AppInfo>,
}

/// Information about a Fly app (from list endpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AppInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub machine_count: usize,
    #[serde(default)]
    pub network: Option<String>,
}

/// Information about a Fly machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MachineInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    pub created_at: String, // ISO 8601: "2023-10-31T02:30:10Z"
}

/// Response from creating an app
/// https://docs.machines.dev/#tag/apps/post/apps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CreateAppResponse {
    pub id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub(crate) enum FlyHost {
    Internal,
    Public,
}

pub(crate) type Error = String;

#[derive(Debug)]
pub(crate) struct FlyApi {
    token: String,
    rate_limiter: governor::RateLimiter<
        governor::state::NotKeyed,
        governor::state::InMemoryState,
        governor::clock::QuantaClock,
        governor::middleware::NoOpMiddleware<governor::clock::QuantaInstant>,
    >,
    http_client: reqwest::Client,
    api_hostname: String,
}

impl FlyApi {
    pub fn new(token: String, http_client: reqwest::Client, host: FlyHost) -> Self {
        let quota = governor::Quota::per_second(nonzero!(1u32)).allow_burst(nonzero!(3u32));
        let rate_limiter = governor::RateLimiter::direct(quota);
        Self {
            token,
            rate_limiter,
            http_client,
            api_hostname: match host {
                FlyHost::Internal => "http://_api.internal:4280".into(),
                FlyHost::Public => "https://api.machines.dev".into(),
            },
        }
    }

    pub async fn create_app(
        &self,
        name: FlyAppName,
        org: FlyOrg,
        network: FlyNetwork,
    ) -> Result<CreateAppResponse, Error> {
        let jitter = governor::Jitter::new(Duration::ZERO, Duration::from_secs(2));
        self.rate_limiter.until_ready_with_jitter(jitter).await;
        let request = CreateAppRequest {
            name,
            org_slug: org,
            network,
        };
        tracing::debug!("Fly create_app request: {:?}", request);
        let host = format!("{}/v1/apps", self.api_hostname);
        let response = self
            .http_client
            .post(&host)
            .bearer_auth(&self.token)
            .json(&request)
            .send()
            .await;
        tracing::info!("Fly create_app response: {:?}", response);
        match response {
            Ok(response) if response.status() == 201 => {
                let app: CreateAppResponse = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse create_app response: {}", e))?;
                Ok(app)
            }
            Ok(response) => Err(format!(
                "Unexpected response status: {}. Message: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )),
            Err(err) => Err(format!("HTTP request failed: {}", err)),
        }
    }

    pub async fn destroy_app(&self, app_name: FlyAppName) -> Result<(), Error> {
        let jitter = governor::Jitter::new(Duration::ZERO, Duration::from_secs(2));
        self.rate_limiter.until_ready_with_jitter(jitter).await;
        tracing::debug!("Fly destroy_app: {}", app_name);
        let host = format!("{}/v1/apps/{}", self.api_hostname, app_name);
        let response = self
            .http_client
            .delete(&host)
            .bearer_auth(&self.token)
            .send()
            .await;
        tracing::info!("Fly destroy_app response: {:?}", response);
        match response {
            Ok(response) if response.status() == 202 => Ok(()),
            Ok(response) => {
                let status = response.status();
                tracing::warn!(
                    "Unexpected response status: {}. Message: {}",
                    status,
                    response.text().await.unwrap_or_default()
                );
                Err(format!("Unexpected response status: {}", status))
            }
            Err(err) => {
                tracing::warn!("HTTP request failed: {}", err);
                Err(format!("HTTP request failed: {}", err))
            }
        }
    }

    pub async fn assign_ip(
        &self,
        app_name: FlyAppName,
        network: FlyNetwork,
        org_slug: FlyOrg,
        service_name: FlyServiceName,
        ip_type: FlyIpType,
    ) -> Result<(), Error> {
        let jitter = governor::Jitter::new(Duration::ZERO, Duration::from_secs(2));
        self.rate_limiter.until_ready_with_jitter(jitter).await;
        let request = AssignIpRequest {
            network,
            org_slug,
            service_name,
            ip_type,
        };
        tracing::debug!("Fly assign_ip request: {:?}", request);
        let host = format!("{}/v1/apps/{}/ip_assignments", self.api_hostname, app_name);
        let response = self
            .http_client
            .post(&host)
            .bearer_auth(&self.token)
            .json(&request)
            .send()
            .await;
        tracing::info!("Fly assign_ip response: {:?}", response);
        match response {
            Ok(response) if response.status() == 200 => Ok(()),
            Ok(response) => {
                let status = response.status();
                tracing::warn!(
                    "Unexpected response status: {}. Message: {}",
                    status,
                    response.text().await.unwrap_or_default()
                );
                Err(format!("Unexpected response status: {}", status))
            }
            Err(err) => {
                tracing::warn!("HTTP request failed: {}", err);
                Err(format!("HTTP request failed: {}", err))
            }
        }
    }

    pub async fn create_machine(
        &self,
        app_name: FlyAppName,
        config: FlyMachineConfig,
    ) -> Result<CreateMachineResponse, Error> {
        let jitter = governor::Jitter::new(Duration::ZERO, Duration::from_secs(2));
        self.rate_limiter.until_ready_with_jitter(jitter).await;
        let request = CreateMachineRequest { config };
        tracing::debug!("Fly create_machine request: {:?}", request);
        let host = format!("{}/v1/apps/{}/machines", self.api_hostname, app_name);
        let response = self
            .http_client
            .post(&host)
            .bearer_auth(&self.token)
            .json(&request)
            .send()
            .await;
        tracing::info!("Fly create_machine response: {:?}", response);
        match response {
            Ok(response) if response.status() == 200 => {
                let machine: CreateMachineResponse = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse create_machine response: {}", e))?;
                Ok(machine)
            }
            Ok(response) => {
                let status = response.status();
                tracing::warn!(
                    "Unexpected response status: {}. Message: {}",
                    status,
                    response.text().await.unwrap_or_default()
                );
                Err(format!("Unexpected response status: {}", status))
            }
            Err(err) => {
                tracing::warn!("HTTP request failed: {}", err);
                Err(format!("HTTP request failed: {}", err))
            }
        }
    }

    pub async fn list_apps(&self, org_slug: FlyOrg) -> Result<ListAppsResponse, Error> {
        let jitter = governor::Jitter::new(Duration::ZERO, Duration::from_secs(2));
        self.rate_limiter.until_ready_with_jitter(jitter).await;
        tracing::debug!("Fly list_apps: org={}", org_slug);
        let host = format!("{}/v1/apps?org_slug={}", self.api_hostname, org_slug);
        let response = self
            .http_client
            .get(&host)
            .bearer_auth(&self.token)
            .send()
            .await;
        tracing::debug!("Fly list_apps response: {:?}", response);
        match response {
            Ok(response) if response.status() == 200 => {
                let apps: ListAppsResponse = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse list_apps response: {}", e))?;
                Ok(apps)
            }
            Ok(response) => {
                let status = response.status();
                tracing::warn!(
                    "Unexpected response status: {}. Message: {}",
                    status,
                    response.text().await.unwrap_or_default()
                );
                Err(format!("Unexpected response status: {}", status))
            }
            Err(err) => {
                tracing::warn!("HTTP request failed: {}", err);
                Err(format!("HTTP request failed: {}", err))
            }
        }
    }

    pub async fn list_machines(&self, app_name: FlyAppName) -> Result<Vec<MachineInfo>, Error> {
        let jitter = governor::Jitter::new(Duration::ZERO, Duration::from_secs(2));
        self.rate_limiter.until_ready_with_jitter(jitter).await;
        tracing::debug!("Fly list_machines: app={}", app_name);
        let host = format!("{}/v1/apps/{}/machines", self.api_hostname, app_name);
        let response = self
            .http_client
            .get(&host)
            .bearer_auth(&self.token)
            .send()
            .await;
        tracing::debug!("Fly list_machines response: {:?}", response);
        match response {
            Ok(response) if response.status() == 200 => {
                let machines: Vec<MachineInfo> = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse list_machines response: {}", e))?;
                Ok(machines)
            }
            Ok(response) => {
                let status = response.status();
                tracing::warn!(
                    "Unexpected response status: {}. Message: {}",
                    status,
                    response.text().await.unwrap_or_default()
                );
                Err(format!("Unexpected response status: {}", status))
            }
            Err(err) => {
                tracing::warn!("HTTP request failed: {}", err);
                Err(format!("HTTP request failed: {}", err))
            }
        }
    }
}
