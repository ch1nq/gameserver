use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type FlyAppName = String;
type FlyNetwork = String;
type FlyOrg = String;
type FlyRegion = String;
type FlyServiceName = String;
type FlyMachineId = String;
type FlyEnv = HashMap<String, String>;
type ImageUrl = String;

/// https://docs.machines.dev/#tag/apps/post/apps
#[derive(Debug, Serialize, Deserialize)]

struct CreateAppRequest {
    name: FlyAppName,
    network: FlyNetwork,
    org_slug: FlyOrg,
}

/// https://docs.machines.dev/#tag/apps/delete/apps/{app_name}
#[derive(Debug, Serialize, Deserialize)]

struct DestroyAppRequest {
    name: FlyAppName,
}

/// https://docs.machines.dev/#tag/apps/post/apps/{app_name}/ip_assignments
#[derive(Debug, Serialize, Deserialize)]
struct AssignIpRequest {
    network: FlyNetwork,
    org_slug: FlyOrg,
    region: FlyRegion,
    service_name: FlyServiceName,
    #[serde(alias = "type")]
    ip_type: FlyIpType,
}

#[derive(Debug, Serialize, Deserialize)]
enum FlyIpType {
    PrivateV6,
}

/// https://docs.machines.dev/#tag/machines/post/apps/{app_name}/machines
#[derive(Debug, Serialize, Deserialize)]
struct CreateMachineRequest {
    // Path parameters:
    app_name: FlyAppName,
    // Body parameters:
    config: FlyMachineConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct FlyMachineConfig {
    image: ImageUrl,
    env: FlyEnv,
    auto_destroy: bool,
    restart: FlyRestartConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct FlyRestartConfig {
    /// When policy is on-failure, the maximum number of times to attempt to restart the Machine before letting it stop.
    max_retries: u32,
    policy: FlyRestartPolicy,
}

#[derive(Debug, Serialize, Deserialize)]
enum FlyRestartPolicy {
    /// Never try to restart a Machine automatically when its main process exits, whether that’s on purpose or on a crash.
    No,
    /// Always restart a Machine automatically and never let it enter a stopped state, even when the main process exits cleanly.
    Always,
    /// Try up to MaxRetries times to automatically restart the Machine if it exits with a non-zero exit code. Default when no explicit policy is set, and for Machines with schedules.
    OnFailure,
    /// Starts the Machine only when there is capacity and the spot price is less than or equal to the bid price.
    SpotPrice,
}

/// https://docs.machines.dev/#tag/machines/post/apps/{app_name}/machines
#[derive(Debug, Serialize, Deserialize)]
struct StartMachineRequest {
    // Path parameters
    app_name: FlyAppName,
    machine_id: FlyMachineId,
}

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
