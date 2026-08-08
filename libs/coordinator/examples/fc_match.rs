//! End-to-end match harness: runs one real game through `GameCoordinator` on the
//! Firecracker backend, with the DB stubbed out.
//!
//! Spawns a game-host VM (gRPC `GameHost` on :50051) + N agent VMs (gRPC `Agent`
//! on :50052), drives one match, prints the result, and tears everything down.
//! Run as root on the Firecracker host:
//!
//!   cargo run -p coordinator --example fc_match
//!
//! Images are pulled the same way production pulls them (no local-import
//! shortcut), so both must be reachable from the FC host.
//!
//! Game host: pulled over HTTPS by ref, no auth
//! (default `ghcr.io/ch1nq/achtung-game-host:latest`).
//!
//! Agents: pulled from the private registry at the provider's `registry_url`
//! (default `http://localhost:5001`) with a scoped token, so a registry must be
//! running there (e.g. docker-compose's `registry` service or a standalone
//! `registry:2`). Push the sample agent to it first — with the defaults that is
//! `docker push localhost:5001/user-0/sample-agent:latest`.
//!
//! Env overrides:
//!   FC_MATCH_HOST_IMAGE   game host image ref  (default: ghcr.io/ch1nq/achtung-game-host:latest)
//!   FC_MATCH_AGENT_IMAGE  agent repo:tag       (default: sample-agent:latest)
//!   FC_MATCH_AGENTS       number of agents     (default: 2)
//!   plus the FIRECRACKER_* / CONTAINERD_SOCKET vars honoured by the provider.

use std::sync::Arc;
use std::time::Duration;

use agent_infra::{FirecrackerMachineProvider, FirecrackerMachineProviderConfig};
use common::{
    AgentImageUrl, AgentInfo, AgentRepository, ContainerImageUrl, DeployTokenProvider,
    RegistryToken,
};
use coordinator::{CoordinatorConfig, GameCoordinator, ImageUrl};

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Stub roster: hands back N sample-agent machines, all sharing one image.
struct StubAgentRepo {
    agent_image: AgentImageUrl,
    count: usize,
}

#[async_trait::async_trait]
impl AgentRepository for StubAgentRepo {
    async fn get_random_active_agents(
        &self,
        count: usize,
    ) -> Result<Vec<AgentInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let n = count.min(self.count);
        Ok((0..n)
            .map(|i| AgentInfo {
                id: (i + 1) as i64,
                image_url: self.agent_image.clone(),
            })
            .collect())
    }
}

/// Stub token provider — the local test registry is unauthenticated, so any
/// token is accepted.
struct StubTokenProvider;

#[async_trait::async_trait]
impl DeployTokenProvider for StubTokenProvider {
    async fn get_deploy_token(
        &self,
        _image: &(dyn ContainerImageUrl + Send + Sync),
    ) -> Result<RegistryToken, Box<dyn std::error::Error + Send + Sync>> {
        Ok(RegistryToken::new("unused".to_string()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "coordinator=debug,agent_infra=info,info".into()),
        )
        .init();

    let host_image = env_or(
        "FC_MATCH_HOST_IMAGE",
        "ghcr.io/ch1nq/achtung-game-host:latest",
    );
    let agent_image = env_or("FC_MATCH_AGENT_IMAGE", "sample-agent:latest");
    let num_agents: usize = env_or("FC_MATCH_AGENTS", "2").parse().unwrap_or(2);

    // Provider config: defaults + the same FIRECRACKER_*/CONTAINERD_SOCKET env
    // overrides the fc_smoke example honours.
    let defaults = FirecrackerMachineProviderConfig::default();
    let provider_cfg = FirecrackerMachineProviderConfig {
        containerd_socket: env_or("CONTAINERD_SOCKET", &defaults.containerd_socket),
        subnet_pool: std::env::var("FIRECRACKER_SUBNET_POOL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(defaults.subnet_pool),
        vcpu_count: std::env::var("FIRECRACKER_VCPU_COUNT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(defaults.vcpu_count),
        mem_size_mib: std::env::var("FIRECRACKER_MEM_SIZE_MIB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(defaults.mem_size_mib),
        ..defaults
    };

    println!("== fc_match ==");
    println!("  host image:  {host_image}");
    println!("  agent image: {agent_image}");
    println!("  agents:      {num_agents}");

    let provider = Arc::new(FirecrackerMachineProvider::new(provider_cfg).await?);

    let config = CoordinatorConfig {
        game_host_image: ImageUrl::new(host_image).expect("valid host image ref"),
        agents_per_game: num_agents,
        tick_rate_ms: 50,
        game_interval: Duration::from_secs(1),
        poll_interval: Duration::from_millis(500),
        game_host_grpc_port: 50051,
        agent_grpc_port: 50052,
    };

    let coordinator = GameCoordinator::new(
        config,
        provider,
        Box::new(StubAgentRepo {
            agent_image: AgentImageUrl::parse(0, &agent_image).expect("valid agent image ref"),
            count: num_agents,
        }),
        Box::new(StubTokenProvider),
        std::sync::Arc::new(tokio::sync::RwLock::new(None)),
    );

    println!("\n[run_once] starting match …");
    match coordinator.run_once().await {
        Ok(()) => {
            println!("\n== fc_match OK (see 'Game finished' log above for placements) ==");
            Ok(())
        }
        Err(e) => {
            eprintln!("\n== fc_match FAILED: {e} ==");
            Err(e.into())
        }
    }
}
