//! Linux network setup for firecracker microVM matches.
//!
//! Each game match gets its own Linux bridge and per-machine TAP devices.
//! iptables rules enforce isolation: agents may only communicate with the
//! game host, not with each other, and have no internet access.
//!
//! # Layout for a match with subnet 10.200.42.0/24
//!
//! ```text
//! Host bridge: br-m-42  (10.200.42.254/24)
//!   ├── tap-m-42-0  → game host microVM   (10.200.42.1)
//!   ├── tap-m-42-1  → agent 1 microVM     (10.200.42.2)
//!   ├── tap-m-42-2  → agent 2 microVM     (10.200.42.3)
//!   └── ...
//! ```
//!
//! # iptables rules
//!
//! - All forwarding on the bridge is dropped by default.
//! - Game host (.1) may send to and receive from any IP on the bridge.
//! - Agents (.2+) may only send to the game host (.1) and receive from it.
//! - No NAT or masquerading → no internet access.

use std::net::Ipv4Addr;

use ipnet::Ipv4Net;
use tokio::process::Command;

/// Name prefix for bridges (max 15 chars total for Linux interface names).
const BRIDGE_PREFIX: &str = "br-m-";
/// Name prefix for TAP devices.
const TAP_PREFIX: &str = "tap-m-";

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("Failed to run network command `{cmd}`: {error}")]
    CommandFailed { cmd: String, error: String },
    #[error("Network command `{cmd}` exited with status {status}: {stderr}")]
    CommandError {
        cmd: String,
        status: i32,
        stderr: String,
    },
}

/// Per-match network context, created by `setup` and destroyed by `teardown`.
#[derive(Debug, Clone)]
pub struct MatchNetwork {
    /// Short identifier derived from match_id, kept for logging/debugging.
    /// (Interface names are keyed off the subnet octet, not this id.)
    pub id: String,
    /// The /24 subnet allocated for this match
    pub subnet: Ipv4Net,
    /// Bridge interface name (e.g., "br-m-abc123")
    pub bridge_name: String,
    /// IP of the host on the bridge (last usable address in the subnet)
    pub host_ip: Ipv4Addr,
    /// IP assigned to the game host microVM (slot 0 → .1)
    pub game_host_ip: Ipv4Addr,
}

impl MatchNetwork {
    /// Compute the IP address for a given slot within the match subnet.
    ///
    /// - slot 0 → game host → `subnet.network() + 1` (e.g., 10.200.42.1)
    /// - slot N → agent N   → `subnet.network() + N + 1` (e.g., 10.200.42.2+)
    pub fn ip_for_slot(&self, slot: u8) -> Ipv4Addr {
        let base = u32::from(self.subnet.network());
        Ipv4Addr::from(base + u32::from(slot) + 1)
    }

    /// The TAP device name for a given slot (e.g., "tap-m-42-0").
    ///
    /// Named off the subnet's third octet, which the subnet pool guarantees is
    /// unique among concurrent matches, so TAP names never collide. Length is
    /// bounded: "tap-m-" (6) + octet (≤3) + "-" (1) + slot (≤3) = ≤13 chars,
    /// within the 15-char Linux interface-name limit.
    pub fn tap_name(&self, slot: u8) -> String {
        format!("{}{}-{}", TAP_PREFIX, self.subnet_octet(), slot)
    }

    /// The subnet's third octet, a unique per-match id from the subnet pool.
    fn subnet_octet(&self) -> u8 {
        self.subnet.network().octets()[2]
    }
}

/// Set up the bridge and iptables rules for a new match.
pub async fn setup(match_id: &str, subnet: Ipv4Net) -> Result<MatchNetwork, NetworkError> {
    // Interface names are keyed off the subnet's third octet, which the subnet
    // pool guarantees is unique among concurrent matches (a truncated match_id
    // prefix could collide and abort the second match on "File exists").
    let id = match_id[..8.min(match_id.len())].to_string();
    let bridge_name = format!("{}{}", BRIDGE_PREFIX, subnet.network().octets()[2]);

    let base = u32::from(subnet.network());
    let host_ip = Ipv4Addr::from(base + 254);
    let game_host_ip = Ipv4Addr::from(base + 1);
    let prefix = subnet.prefix_len();

    // 1. Create bridge
    run("ip", &["link", "add", &bridge_name, "type", "bridge"]).await?;
    // 2. Assign host IP to bridge
    let cidr = format!("{}/{}", host_ip, prefix);
    run("ip", &["addr", "add", &cidr, "dev", &bridge_name]).await?;
    // 3. Bring bridge up
    run("ip", &["link", "set", &bridge_name, "up"]).await?;

    let network = MatchNetwork {
        id,
        subnet,
        bridge_name: bridge_name.clone(),
        host_ip,
        game_host_ip,
    };

    // 4. Set up iptables isolation rules
    setup_iptables(&network).await?;

    tracing::info!(
        bridge = bridge_name,
        subnet = %subnet,
        host_ip = %host_ip,
        "Match network set up"
    );

    Ok(network)
}

/// Create a TAP device for a microVM slot and attach it to the match bridge.
///
/// Returns the TAP device name.
pub async fn create_tap(network: &MatchNetwork, slot: u8) -> Result<String, NetworkError> {
    let tap_name = network.tap_name(slot);

    // Create TAP device
    run("ip", &["tuntap", "add", &tap_name, "mode", "tap"]).await?;
    // Attach to the match bridge
    run(
        "ip",
        &["link", "set", &tap_name, "master", &network.bridge_name],
    )
    .await?;
    // Bring it up
    run("ip", &["link", "set", &tap_name, "up"]).await?;

    tracing::debug!(tap = tap_name, slot, "TAP device created");
    Ok(tap_name)
}

/// Delete a TAP device created by `create_tap`.
pub async fn delete_tap(tap_name: &str) -> Result<(), NetworkError> {
    run("ip", &["link", "delete", tap_name]).await?;
    tracing::debug!(tap = tap_name, "TAP device deleted");
    Ok(())
}

/// Tear down the match bridge and all associated iptables rules.
///
/// TAP devices should be deleted (via `delete_tap`) before calling this.
pub async fn teardown(network: &MatchNetwork) -> Result<(), NetworkError> {
    // Remove iptables rules first
    teardown_iptables(network).await?;

    // Delete bridge (any remaining TAPs are auto-detached)
    run("ip", &["link", "delete", &network.bridge_name]).await?;

    tracing::info!(bridge = network.bridge_name, "Match network torn down");
    Ok(())
}

/// Install iptables rules for match isolation.
///
/// Policy:
/// - Default DROP on the bridge for forwarding
/// - Game host ↔ any IP on the bridge: ACCEPT
/// - Agent → game host: ACCEPT (for gRPC replies)
/// - Agent → agent: DROP (enforced by absence of an ACCEPT rule)
/// - No NAT → no internet access
fn setup_iptables_rules(network: &MatchNetwork) -> Vec<Vec<String>> {
    let bridge = &network.bridge_name;
    let game_host_ip = network.game_host_ip.to_string();

    vec![
        // Drop all forwarding into the bridge by default
        ipt(&["-I", "FORWARD", "-o", bridge, "-j", "DROP"]),
        // Drop all forwarding out of the bridge by default
        ipt(&["-I", "FORWARD", "-i", bridge, "-j", "DROP"]),
        // Allow game host to send to anyone on the bridge
        ipt(&[
            "-I",
            "FORWARD",
            "-i",
            bridge,
            "-s",
            &game_host_ip,
            "-j",
            "ACCEPT",
        ]),
        // Allow anyone on the bridge to send to game host
        ipt(&[
            "-I",
            "FORWARD",
            "-i",
            bridge,
            "-d",
            &game_host_ip,
            "-j",
            "ACCEPT",
        ]),
        // Allow coordinator (on host) → game host: routed through bridge.
        // Replies (game host → coordinator) are already covered by the
        // "-i bridge -s game_host_ip ACCEPT" rule above.
        ipt(&[
            "-I",
            "FORWARD",
            "-o",
            bridge,
            "-d",
            &game_host_ip,
            "-j",
            "ACCEPT",
        ]),
    ]
}

async fn setup_iptables(network: &MatchNetwork) -> Result<(), NetworkError> {
    for args in setup_iptables_rules(network) {
        run(
            "iptables",
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await?;
    }
    Ok(())
}

async fn teardown_iptables(network: &MatchNetwork) -> Result<(), NetworkError> {
    // Mirror of setup_iptables_rules but with -D (delete) instead of -I (insert)
    for mut args in setup_iptables_rules(network) {
        // Replace "-I" with "-D"
        if let Some(flag) = args.iter_mut().find(|a| a.as_str() == "-I") {
            *flag = "-D".to_string();
        }
        // Best-effort: log failures but don't abort
        if let Err(e) = run(
            "iptables",
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await
        {
            tracing::warn!(error = %e, "Failed to remove iptables rule (may have already been removed)");
        }
    }
    Ok(())
}

fn ipt(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

/// Run a system command, returning an error if it fails.
async fn run(program: &str, args: &[&str]) -> Result<(), NetworkError> {
    let cmd_str = format!("{} {}", program, args.join(" "));
    let output = Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(|e| NetworkError::CommandFailed {
            cmd: cmd_str.clone(),
            error: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(NetworkError::CommandError {
            cmd: cmd_str,
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(())
}
