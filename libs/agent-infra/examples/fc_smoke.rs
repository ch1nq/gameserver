//! Standalone smoke test for the Firecracker backend.
//!
//! Drives the `MachineProvider` trait directly against a real
//! firecracker-containerd daemon — no website / Postgres / registry needed. It
//! exercises the three paths that can only be validated on a KVM host:
//!   * `CreateVM` field plumbing (static network config on a pre-created TAP),
//!   * `ctr run` + the `aws.firecracker.vm.id` annotation binding the container
//!     into that VM,
//!   * the devmapper snapshot → VM rootfs chain.
//!
//! Must run as root on the host (it creates bridges/TAPs, runs iptables, and
//! talks to the firecracker-containerd socket).
//!
//! Usage:
//!   cargo run -p achtung-agent-infra --example fc_smoke
//!
//! Env overrides (all optional):
//!   FC_SMOKE_IMAGE     OCI image to boot   (default: ghcr.io/ch1nq/achtung-game-host:latest)
//!   FC_SMOKE_MACHINES  how many machines   (default: 1; use >=3 to eyeball isolation)
//!   FC_SMOKE_PORT      TCP port to probe   (default: 50051, the game host gRPC port)
//!   FC_SMOKE_KEEP      if set, skip teardown so you can poke at the VMs
//!   CONTAINERD_SOCKET  firecracker-containerd socket
//!   FIRECRACKER_KERNEL kernel image path (informational; runtime.json is authoritative)
//!   FIRECRACKER_SUBNET_POOL, FIRECRACKER_VCPU_COUNT, FIRECRACKER_MEM_SIZE_MIB

use std::time::Duration;

use agent_infra::{
    ContainerImage, FirecrackerMachineProvider, FirecrackerMachineProviderConfig, MachineHandle,
    MachineProvider, SpawnConfig,
};
use common::ImageUrl;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn config_from_env() -> FirecrackerMachineProviderConfig {
    let defaults = FirecrackerMachineProviderConfig::default();
    FirecrackerMachineProviderConfig {
        containerd_socket: env_or("CONTAINERD_SOCKET", &defaults.containerd_socket),
        kernel_path: env_or("FIRECRACKER_KERNEL", &defaults.kernel_path),
        vcpu_count: std::env::var("FIRECRACKER_VCPU_COUNT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(defaults.vcpu_count),
        mem_size_mib: std::env::var("FIRECRACKER_MEM_SIZE_MIB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(defaults.mem_size_mib),
        subnet_pool: std::env::var("FIRECRACKER_SUBNET_POOL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(defaults.subnet_pool),
        ..defaults
    }
}

/// Best-effort reachability check from the host bridge IP into the guest.
async fn probe(ip: &str, port: &str) {
    // ICMP first: proves the guest booted and statically configured its NIC.
    let ping = tokio::process::Command::new("ping")
        .args(["-c", "3", "-W", "2", ip])
        .output()
        .await;
    match ping {
        Ok(o) if o.status.success() => println!("  ✓ ping {ip} OK"),
        Ok(o) => println!(
            "  ✗ ping {ip} failed:\n{}",
            String::from_utf8_lossy(&o.stdout)
        ),
        Err(e) => println!("  ✗ ping {ip} could not run: {e}"),
    }

    // Then the app port, retried while the container process comes up.
    let addr = format!("{ip}:{port}");
    for attempt in 1..=15 {
        let connect = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(&addr),
        )
        .await;
        match connect {
            Ok(Ok(_)) => {
                println!("  ✓ TCP {addr} open (attempt {attempt})");
                return;
            }
            _ => tokio::time::sleep(Duration::from_secs(2)).await,
        }
    }
    println!("  ✗ TCP {addr} never opened (app may need env, or port differs)");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agent_infra=debug,info".into()),
        )
        .init();

    let image = env_or("FC_SMOKE_IMAGE", "ghcr.io/ch1nq/achtung-game-host:latest");
    let machines: u8 = env_or("FC_SMOKE_MACHINES", "1").parse().unwrap_or(1);
    let port = env_or("FC_SMOKE_PORT", "50051");
    let keep = std::env::var("FC_SMOKE_KEEP").is_ok();

    let config = config_from_env();
    println!("== fc_smoke ==");
    println!("  socket:  {}", config.containerd_socket);
    println!("  subnet:  {}", config.subnet_pool);
    println!(
        "  vm size: {} vcpu / {} MiB",
        config.vcpu_count, config.mem_size_mib
    );
    println!("  image:   {image}");
    println!("  machines:{machines}  probe port:{port}  keep:{keep}");

    let provider = FirecrackerMachineProvider::new(config).await?;

    let match_id = agent_infra::generate_id();
    println!("\n[init_match] match_id={match_id}");
    let ctx = provider.init_match(&match_id, machines).await?;

    let mut handles: Vec<MachineHandle> = Vec::new();
    let mut spawn_err = None;
    for slot in 0..machines {
        println!("\n[spawn] slot {slot} …");
        let cfg = SpawnConfig::new(ContainerImage::Public(ImageUrl::from(image.clone())), slot)
            .env("SLOT", slot.to_string());
        match provider.spawn(&ctx, cfg).await {
            Ok(handle) => {
                println!(
                    "  spawned: id={} ip={}",
                    handle.machine_id, handle.private_ip
                );
                let ip = handle.private_ip.clone();
                let port = port.clone();
                probe(&ip, &port).await;
                handles.push(handle);
            }
            Err(e) => {
                println!("  ✗ spawn slot {slot} failed: {e}");
                spawn_err = Some(e);
                break;
            }
        }
    }

    if keep {
        println!(
            "\n[keep] leaving {} machine(s) running. Clean up later with:",
            handles.len()
        );
        println!("       sudo ./scripts/setup-firecracker/uninstall.sh");
        if let Some(e) = spawn_err {
            return Err(e.into());
        }
        return Ok(());
    }

    println!("\n[destroy] tearing down {} machine(s) …", handles.len());
    for handle in &handles {
        if let Err(e) = provider.destroy(&ctx, handle).await {
            println!("  ✗ destroy {} failed: {e}", handle.machine_id);
        } else {
            println!("  ✓ destroyed {}", handle.machine_id);
        }
    }

    println!("[cleanup_match] releasing subnet + bridge …");
    provider.cleanup_match(ctx).await?;
    println!("  ✓ cleanup complete");

    match spawn_err {
        Some(e) => Err(e.into()),
        None => {
            println!("\n== fc_smoke OK ==");
            Ok(())
        }
    }
}
