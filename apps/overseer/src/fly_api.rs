use nonzero_ext::nonzero;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};

type FlyAppName = String;
type FlyNetwork = String;
type FlyOrg = String;
type FlyServiceName = String;
type FlyMachineId = String;
type FlyEnv = HashMap<String, String>;
type ImageUrl = String;

/// https://docs.machines.dev/#tag/apps/post/apps
#[derive(Debug, Serialize, Deserialize)]

struct CreateAppRequest {
    name: FlyAppName,
    org_slug: FlyOrg,
    network: FlyNetwork,
}

type CreateAppResponse = ();

/// https://docs.machines.dev/#tag/apps/delete/apps/{app_name}
#[derive(Debug, Serialize, Deserialize)]

struct DestroyAppRequest {
    name: FlyAppName,
}

type DestroyAppResponse = ();

/// https://docs.machines.dev/#tag/apps/post/apps/{app_name}/ip_assignments
#[derive(Debug, Serialize, Deserialize)]
struct AssignIpRequest {
    network: FlyNetwork,
    org_slug: FlyOrg,
    service_name: FlyServiceName,
    #[serde(rename = "type")]
    ip_type: FlyIpType,
}

type AssignIpResponse = ();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlyIpType {
    PrivateV6,
}

/// https://docs.machines.dev/#tag/machines/post/apps/{app_name}/machines
#[derive(Debug, Serialize, Deserialize)]
struct CreateMachineRequest {
    config: FlyMachineConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FlyMachineConfig {
    pub image: ImageUrl,
    pub env: FlyEnv,
    pub auto_destroy: bool,
    pub restart: FlyRestartConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FlyRestartConfig {
    /// When policy is on-failure, the maximum number of times to attempt to restart the Machine before letting it stop.
    pub max_retries: u32,
    pub policy: FlyRestartPolicy,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlyRestartPolicy {
    /// Never try to restart a Machine automatically when its main process exits, whether that’s on purpose or on a crash.
    No,
    /// Always restart a Machine automatically and never let it enter a stopped state, even when the main process exits cleanly.
    Always,
    /// Try up to MaxRetries times to automatically restart the Machine if it exits with a non-zero exit code. Default when no explicit policy is set, and for Machines with schedules.
    OnFailure,
    /// Starts the Machine only when there is capacity and the spot price is less than or equal to the bid price.
    SpotPrice,
}

type CreateMachineResponse = ();

/// https://docs.machines.dev/#tag/machines/post/apps/{app_name}/machines
#[derive(Debug, Serialize, Deserialize)]
struct StartMachineRequest {
    // Path parameters
    app_name: FlyAppName,
    machine_id: FlyMachineId,
}

type StartMachineResponse = ();

/// https://docs.machines.dev/#tag/machines/post/apps/{app_name}/machines
#[derive(Debug, Serialize, Deserialize)]
struct StopMachineRequest {
    // Path parameters
    app_name: FlyAppName,
    machine_id: FlyMachineId,
    // Body parameters
    signal: StopSignal,
    timeout: StopTimeout,
}

#[derive(Debug, Serialize, Deserialize)]
enum StopSignal {
    SIGTERM,
}

#[derive(Debug, Serialize, Deserialize)]
struct StopTimeout {
    #[serde(alias = "time.Duration")]
    duration: u64,
}

type StopMachineResponse = ();

#[derive(Debug, Clone)]
pub enum FlyHost {
    Internal,
    Public,
}

#[derive(Debug)]
pub struct FlyApi {
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

type Error = String;

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
    ) -> Result<(), Error> {
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
            Ok(response) if response.status() == 201 => Ok(()),
            Ok(response) => Err(format!(
                "Unexpected response status: {}. Message: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )),
            Err(err) => Err(format!("HTTP request failed: {}", err)),
        }
    }

    pub async fn destroy_app(&self, request: DestroyAppRequest) -> DestroyAppResponse {
        todo!()
    }

    pub async fn assign_ip(
        &self,
        app_name: FlyAppName,
        network: FlyNetwork,
        org_slug: FlyOrg,
        service_name: FlyServiceName,
        ip_type: FlyIpType,
    ) -> Result<AssignIpResponse, Error> {
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

    pub async fn start_machine(&self, request: StartMachineRequest) -> StartMachineResponse {
        todo!()
    }
    pub async fn stop_machine(&self, request: StopMachineRequest) -> StopMachineResponse {
        todo!()
    }
}
