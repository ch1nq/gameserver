//! Smoke test for the per-match-network Docker isolation mode (gVisor design).
//!
//! Spawns a fake match (1 game host + 2 agents) through the real
//! `DockerMachineProvider` in `PerMatchNetworks` mode, then asserts the
//! isolation matrix from docs/plans/gvisor-migration.md:
//!
//!   1. host → game host TCP: OPEN      (coordinator dials the game host)
//!   2. agent → game host:    OPEN      (game traffic)
//!   3. game host → agent:    OPEN      (game host dials agent gRPC)
//!   4. agent → agent:        BLOCKED   (no shared network)
//!   5. agent → internet:     BLOCKED   (internal networks)
//!   6. agent → host gateway: reported  (BLOCKED once the static INPUT rules
//!                                       from scripts/setup-gvisor are in place;
//!                                       reported as WARN, not FAIL, without them)
//!
//! Usage:
//!   cargo run -p agent-infra --example gvisor_smoke                 # runtime: runc
//!   GVISOR_SMOKE_RUNTIME=runsc cargo run -p agent-infra --example gvisor_smoke
//!
//! `runc` exercises the network topology only; `runsc` is the production
//! configuration. Exits non-zero if any hard assertion fails.
//!
//! Builds a tiny local test image (alpine + idle command) on first run; match
//! containers are then spawned from it via the provider like real agents.

use agent_infra::{
    ContainerImage, DockerIsolation, DockerMachineProvider, DockerMachineProviderConfig,
    MachineHandle, MachineProvider, SpawnConfig,
};
use common::ImageUrl;

const AGENTS: u8 = 2;
const TEST_IMAGE: &str = "achtung-gvisor-smoke:latest";
const PORT: u16 = 7777;

/// Build the idle test image if it doesn't exist locally. The provider's
/// `Public` image path pulls only if absent, so the local tag is used as-is.
async fn ensure_test_image() {
    let dockerfile = "FROM alpine:latest\nCMD [\"sleep\", \"infinity\"]\n";
    let mut child = tokio::process::Command::new("docker")
        .args(["build", "-q", "-t", TEST_IMAGE, "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("failed to run docker build");
    use tokio::io::AsyncWriteExt;
    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(dockerfile.as_bytes())
        .await
        .expect("write dockerfile");
    let status = child.wait().await.expect("docker build wait");
    assert!(status.success(), "docker build of {TEST_IMAGE} failed");
}

/// Run a command inside a container via `docker exec`, returning success.
async fn exec_ok(container: &str, cmd: &[&str]) -> bool {
    let output = tokio::process::Command::new("docker")
        .arg("exec")
        .arg(container)
        .args(cmd)
        .output()
        .await
        .expect("failed to run docker exec");
    output.status.success()
}

/// `docker inspect` with a Go template, trimmed stdout.
async fn inspect(target: &str, format: &str) -> String {
    let out = tokio::process::Command::new("docker")
        .args(["inspect", "-f", format, target])
        .output()
        .await
        .expect("docker inspect failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// TCP probe from inside a container (busybox nc).
async fn probe_from(container: &str, ip: &str, port: u16) -> bool {
    let cmd = format!("nc -w 2 {ip} {port} </dev/null");
    exec_ok(container, &["sh", "-c", &cmd]).await
}

/// TCP probe from the host (bash /dev/tcp, no nc dependency).
async fn probe_from_host(ip: &str, port: u16) -> bool {
    tokio::process::Command::new("timeout")
        .args(["2", "bash", "-c", &format!("exec 3<>/dev/tcp/{ip}/{port}")])
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

struct Checks {
    failures: Vec<String>,
    warnings: Vec<String>,
}

impl Checks {
    fn assert(&mut self, name: &str, expected_open: bool, actually_open: bool) {
        let verdict = match (expected_open, actually_open) {
            (true, true) => "PASS (open)",
            (false, false) => "PASS (blocked)",
            (true, false) => {
                self.failures.push(name.to_string());
                "FAIL (expected open, was blocked)"
            }
            (false, true) => {
                self.failures.push(name.to_string());
                "FAIL (expected blocked, was open)"
            }
        };
        println!("  {name}: {verdict}");
    }

    fn warn_if_open(&mut self, name: &str, actually_open: bool) {
        if actually_open {
            self.warnings.push(name.to_string());
            println!("  {name}: WARN (open — install the static INPUT rules before production)");
        } else {
            println!("  {name}: PASS (blocked)");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agent_infra=info,warn".into()),
        )
        .init();

    let runtime = std::env::var("GVISOR_SMOKE_RUNTIME").unwrap_or_else(|_| "runc".into());
    ensure_test_image().await;

    let config = DockerMachineProviderConfig {
        isolation: DockerIsolation::PerMatchNetworks {
            runtime: runtime.clone(),
            nano_cpus: 500_000_000,
            memory_bytes: 128 * 1024 * 1024,
            pids_limit: 64,
        },
        registry_pull_host: "localhost:5001".into(),
    };
    let provider = DockerMachineProvider::new(config)?;

    let match_id = agent_infra::generate_id();
    println!("== gvisor_smoke ==  runtime={runtime} match={match_id}");

    let ctx = provider.init_match(&match_id, AGENTS + 1).await?;

    // Spawn game host (slot 0) then agents, like the coordinator does.
    let mut handles: Vec<MachineHandle> = Vec::new();
    for slot in 0..=AGENTS {
        // The in-machine listen port. Docker ignores it (containers are
        // addressed directly); it exists for backends that publish a host port.
        let cfg = SpawnConfig::new(
            ContainerImage::Public(ImageUrl::from(TEST_IMAGE.to_string())),
            slot,
            PORT,
        );
        match provider.spawn(&ctx, cfg).await {
            Ok(handle) => {
                println!(
                    "  spawned slot {slot}: id={} ip={}",
                    handle.machine_id, handle.private_ip
                );
                handles.push(handle);
            }
            Err(e) => {
                eprintln!("spawn slot {slot} failed: {e}");
                cleanup(&provider, ctx, &handles).await;
                return Err(e.into());
            }
        }
    }

    // Every container runs a restarting TCP listener so all matrix directions
    // have a real port to hit (busybox nc has no -k; loop instead). Note the
    // listener runs with the container's limits — including the agents'
    // read-only rootfs — so this also smoke-tests that exec still works there.
    for h in &handles {
        let listener = format!(
            "nohup sh -c 'while true; do nc -l -p {PORT} -e /bin/true; done' >/dev/null 2>&1 &"
        );
        assert!(
            exec_ok(&h.machine_id, &["sh", "-c", &listener]).await,
            "failed to start listener in {}",
            h.machine_id
        );
    }
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let gh = &handles[0];
    let a1 = &handles[1];
    let a2 = &handles[2];

    // The game host has one IP per slot network; agent n reaches it via its IP
    // on network s{n} (which is what the coordinator's env will carry), not via
    // the slot-0 handle IP.
    let gh_container = format!("achtung-{match_id}-slot-0");
    let gh_ip_s1 = inspect(
        &gh_container,
        &format!("{{{{(index .NetworkSettings.Networks \"achtung-{match_id}-s1\").IPAddress}}}}"),
    )
    .await;
    let gateway_s1 = inspect(
        &format!("achtung-{match_id}-s1"),
        "{{(index .IPAM.Config 0).Gateway}}",
    )
    .await;
    println!(
        "  (game host: s0={} s1={gh_ip_s1}; s1 gateway={gateway_s1})",
        gh.private_ip
    );

    let mut checks = Checks {
        failures: Vec::new(),
        warnings: Vec::new(),
    };

    println!("\n[isolation matrix]");
    checks.assert(
        "host->game-host",
        true,
        probe_from_host(&gh.private_ip, PORT).await,
    );
    checks.assert(
        "agent1->game-host",
        true,
        probe_from(&a1.machine_id, &gh_ip_s1, PORT).await,
    );
    checks.assert(
        "game-host->agent1",
        true,
        probe_from(&gh.machine_id, &a1.private_ip, PORT).await,
    );
    checks.assert(
        "agent1->agent2",
        false,
        probe_from(&a1.machine_id, &a2.private_ip, PORT).await,
    );
    checks.assert(
        "agent2->agent1",
        false,
        probe_from(&a2.machine_id, &a1.private_ip, PORT).await,
    );
    checks.assert(
        "agent1->internet",
        false,
        exec_ok(
            &a1.machine_id,
            &["sh", "-c", "nc -w 3 1.1.1.1 443 </dev/null"],
        )
        .await,
    );
    // Gateway SSH as the guest→host canary; WARN until the static INPUT rules
    // from scripts/setup-gvisor are installed on this host.
    checks.warn_if_open(
        "agent1->host-gateway:22",
        exec_ok(
            &a1.machine_id,
            &["sh", "-c", &format!("nc -w 2 {gateway_s1} 22 </dev/null")],
        )
        .await,
    );

    println!("\n[cleanup]");
    cleanup(&provider, ctx, &handles).await;

    if !checks.warnings.is_empty() {
        println!("warnings: {:?}", checks.warnings);
    }
    if checks.failures.is_empty() {
        println!("gvisor_smoke: ALL PASS (runtime={runtime})");
        Ok(())
    } else {
        println!("gvisor_smoke: FAILED: {:?}", checks.failures);
        std::process::exit(1);
    }
}

async fn cleanup(
    provider: &DockerMachineProvider,
    ctx: <DockerMachineProvider as MachineProvider>::MatchContext,
    handles: &[MachineHandle],
) {
    for h in handles {
        if let Err(e) = provider.destroy(&ctx, h).await {
            eprintln!("destroy {}: {e}", h.machine_id);
        }
    }
    if let Err(e) = provider.cleanup_match(ctx).await {
        eprintln!("cleanup_match: {e}");
    }
}
