//! Linux network setup for firecracker microVM matches.
//!
//! Each game match gets its own Linux bridge and per-machine TAP devices.
//! Isolation is enforced in two layers, because guests are untrusted (agents
//! run user-submitted code as root inside their VM and fully control their own
//! network stack — they can change their IP/MAC, use IPv6 link-local addresses,
//! or emit raw Ethernet frames):
//!
//! - **L2 (bridge port isolation):** agent TAPs are `isolated` bridge ports.
//!   Isolated ports cannot exchange *any* frames with each other, regardless
//!   of protocol or what addresses the guest claims. Only the game-host TAP
//!   (slot 0) is non-isolated, so agent↔game-host traffic still flows.
//! - **L3 (iptables/ip6tables):** default-drop FORWARD on the bridge with
//!   game-host-only exceptions, default-drop INPUT from the bridge so guests
//!   cannot reach host services, and a wholesale IPv6 drop (matches are
//!   IPv4-only). Requires `br_netfilter` with `bridge-nf-call-iptables` and
//!   `bridge-nf-call-ip6tables` enabled (install.sh does this).
//!
//! # Layout for a match with subnet 10.200.42.0/24
//!
//! ```text
//! Host bridge: br-m-42  (10.200.42.254/24)
//!   ├── tap-m-42-0  → game host microVM   (10.200.42.1)
//!   ├── tap-m-42-1  → agent 1 microVM     (10.200.42.2)  [isolated port]
//!   ├── tap-m-42-2  → agent 2 microVM     (10.200.42.3)  [isolated port]
//!   └── ...
//! ```
//!
//! # Firewall rules
//!
//! - All forwarding on the bridge is dropped by default.
//! - Game host (.1) may send to and receive from any IP **on its own bridge**.
//! - Agents (.2+) may only send to the game host (.1) and receive from it.
//! - Guests may not initiate connections to the host; only replies to
//!   host-initiated connections (coordinator → game host gRPC) are accepted.
//! - All IPv6 to/from the bridge is dropped.
//! - No NAT or masquerading → no internet access.
//!
//! Residual risk (accepted): an agent can still ARP-spoof toward the game
//! host and MITM *game-host↔agent* traffic within its own match. The game
//! host is trusted, admin-controlled code, so this only affects match
//! integrity for the attacking agent's own match, not other users' matches
//! or the host.

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
/// `owner_uid`/`owner_gid` is the identity the jailed Firecracker VMM runs as:
/// the jailed process has no capabilities, so it can only attach to a TAP whose
/// owner matches. Pass 0 (root) only when jailing is disabled.
///
/// Returns the TAP device name.
pub async fn create_tap(
    network: &MatchNetwork,
    slot: u8,
    owner_uid: u32,
    owner_gid: u32,
) -> Result<String, NetworkError> {
    let tap_name = network.tap_name(slot);

    // Create TAP device, owned by the jailer identity
    let uid = owner_uid.to_string();
    let gid = owner_gid.to_string();
    let mut add_args = vec!["tuntap", "add", tap_name.as_str(), "mode", "tap"];
    if owner_uid != 0 {
        add_args.extend(["user", &uid, "group", &gid]);
    }
    run("ip", &add_args).await?;
    // Attach to the match bridge
    run(
        "ip",
        &["link", "set", &tap_name, "master", &network.bridge_name],
    )
    .await?;
    // Agent TAPs become *isolated* bridge ports: isolated ports cannot
    // exchange any frames with other isolated ports, blocking agent↔agent
    // traffic at L2 (IPv4, IPv6 link-local, ARP spoofing, raw ethertypes
    // alike) no matter what addresses the guest assigns itself. The
    // game-host TAP (slot 0) stays non-isolated so agents can reach it,
    // and the bridge device itself is not a port, so host↔guest traffic
    // is unaffected.
    if slot != 0 {
        run(
            "bridge",
            &["link", "set", "dev", &tap_name, "isolated", "on"],
        )
        .await?;
    }
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

/// A firewall rule: the binary to invoke plus its arguments.
type FirewallRule = (&'static str, Vec<String>);

/// Firewall rules for match isolation (see module docs for the full policy).
///
/// Rules are installed with `-I` (insert at head) in list order, so **later
/// entries end up earlier in the chain**: keep DROPs first and their ACCEPT
/// exceptions after them.
fn setup_iptables_rules(network: &MatchNetwork) -> Vec<FirewallRule> {
    let bridge = &network.bridge_name;
    let game_host_ip = network.game_host_ip.to_string();

    vec![
        // Drop all forwarding into the bridge by default
        (
            "iptables",
            ipt(&["-I", "FORWARD", "-o", bridge, "-j", "DROP"]),
        ),
        // Drop all forwarding out of the bridge by default
        (
            "iptables",
            ipt(&["-I", "FORWARD", "-i", bridge, "-j", "DROP"]),
        ),
        // Allow game host to send to anyone on its own bridge. The `-o bridge`
        // match is load-bearing: without it the game host could route out to
        // other matches' bridges or the host's other networks.
        (
            "iptables",
            ipt(&[
                "-I",
                "FORWARD",
                "-i",
                bridge,
                "-o",
                bridge,
                "-s",
                &game_host_ip,
                "-j",
                "ACCEPT",
            ]),
        ),
        // Allow anyone on the bridge to send to the game host (same bridge only)
        (
            "iptables",
            ipt(&[
                "-I",
                "FORWARD",
                "-i",
                bridge,
                "-o",
                bridge,
                "-d",
                &game_host_ip,
                "-j",
                "ACCEPT",
            ]),
        ),
        // NOTE: no FORWARD rule is needed for coordinator → game host. The
        // coordinator runs on the host itself, and host-originated traffic
        // traverses OUTPUT, never FORWARD. (An earlier unrestricted
        // "-o bridge -d game_host ACCEPT" rule here let VMs of *other*
        // matches reach this match's game host cross-bridge.)
        //
        // Guests may not initiate connections to host services (registry,
        // SSH, coordinator, …). Only replies to host-initiated connections
        // (coordinator → game host gRPC) are allowed back in.
        (
            "iptables",
            ipt(&["-I", "INPUT", "-i", bridge, "-j", "DROP"]),
        ),
        (
            "iptables",
            ipt(&[
                "-I",
                "INPUT",
                "-i",
                bridge,
                "-m",
                "conntrack",
                "--ctstate",
                "RELATED,ESTABLISHED",
                "-j",
                "ACCEPT",
            ]),
        ),
        // Matches are IPv4-only: drop IPv6 wholesale so guests can't bypass
        // the IPv4 rules via link-local addresses. Only effective with
        // net.bridge.bridge-nf-call-ip6tables=1 (set by install.sh).
        (
            "ip6tables",
            ipt(&["-I", "FORWARD", "-o", bridge, "-j", "DROP"]),
        ),
        (
            "ip6tables",
            ipt(&["-I", "FORWARD", "-i", bridge, "-j", "DROP"]),
        ),
        (
            "ip6tables",
            ipt(&["-I", "INPUT", "-i", bridge, "-j", "DROP"]),
        ),
    ]
}

async fn setup_iptables(network: &MatchNetwork) -> Result<(), NetworkError> {
    for (program, args) in setup_iptables_rules(network) {
        run(
            program,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await?;
    }
    Ok(())
}

async fn teardown_iptables(network: &MatchNetwork) -> Result<(), NetworkError> {
    // Mirror of setup_iptables_rules but with -D (delete) instead of -I (insert)
    for (program, mut args) in setup_iptables_rules(network) {
        // Replace "-I" with "-D"
        if let Some(flag) = args.iter_mut().find(|a| a.as_str() == "-I") {
            *flag = "-D".to_string();
        }
        // Best-effort: log failures but don't abort
        if let Err(e) = run(
            program,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await
        {
            tracing::warn!(error = %e, "Failed to remove firewall rule (may have already been removed)");
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
