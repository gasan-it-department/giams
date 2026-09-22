//! Finding the accounting office server on the local network.
//!
//! The counterpart to `discovery.rs` in the node. A workstation browses for
//! `_giams._tcp.local` and reports what it finds, so nobody is ever asked for
//! an IP address — and a server that moves to a different machine is picked up
//! without visiting every desk.
//!
//! Runs entirely on the LAN. An office whose internet line is down but whose
//! router is up discovers the node exactly as normal, which is the behaviour
//! docs/ARCHITECTURE.md §2 requires.

use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::{Deserialize, Serialize};

const SERVICE_TYPE: &str = "_giams._tcp.local.";

/// How long to listen before answering.
///
/// mDNS replies arrive over a spread of a second or so on a quiet network.
/// Three seconds is long enough to catch a slow responder and short enough
/// that a person watching a "looking for the server" message does not give up.
const BROWSE_FOR: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredNode {
    /// What to point the client at, e.g. `http://192.168.1.50:8787`.
    pub url: String,
    pub address: String,
    pub port: u16,
    /// From the TXT records the node advertises.
    pub node_code: String,
    pub municipality: String,
    pub tenant: String,
    pub version: String,
}

/// Browses the local network for accounting servers.
///
/// Returns every node found rather than picking one. On a shared municipal
/// network there may legitimately be more than one, and silently choosing
/// would eventually connect somebody to the wrong municipality's books.
pub fn find_nodes() -> Result<Vec<DiscoveredNode>, String> {
    let daemon = ServiceDaemon::new()
        .map_err(|e| {
            tracing::error!(error = ?e, "could not start network discovery");
            "This computer could not search the network.".to_string()
        })?;

    let receiver = daemon
        .browse(SERVICE_TYPE)
        .map_err(|e| {
            tracing::error!(error = ?e, "could not browse for accounting servers");
            "This computer could not search the network.".to_string()
        })?;

    let deadline = std::time::Instant::now() + BROWSE_FOR;
    let mut found: Vec<DiscoveredNode> = Vec::new();

    while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let Some(address) = info.get_addresses().iter().next().map(|a| a.to_string())
                else {
                    continue;
                };

                let property = |key: &str| {
                    info.get_property_val_str(key).unwrap_or_default().to_string()
                };

                let node = DiscoveredNode {
                    url: format!("http://{address}:{}", info.get_port()),
                    address,
                    port: info.get_port(),
                    node_code: property("node"),
                    municipality: property("municipality"),
                    tenant: property("tenant"),
                    version: property("version"),
                };

                // A node can answer on several interfaces; report it once.
                if !found.iter().any(|n| n.url == node.url) {
                    found.push(node);
                }
            }
            Ok(_) => continue,
            // Timed out with nothing more to hear, or the daemon closed.
            Err(_) => break,
        }
    }

    let _ = daemon.shutdown();

    tracing::info!(count = found.len(), "finished searching for accounting servers");
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Searching a network with no node must be an empty answer, not a failure.
    ///
    /// "No server found" is an ordinary state — the office server is switched
    /// off, or this laptop is at home — and the client shows it as working
    /// offline rather than as something being broken.
    #[test]
    fn finding_nothing_is_not_an_error() {
        let found = find_nodes().expect("discovery failed instead of returning nothing");

        // Prints what was on the network when this ran, so the same test
        // doubles as the check that a live node is discoverable:
        //     cargo test -p giams-desktop discovery -- --nocapture
        for node in &found {
            println!(
                "  found {} at {} — {} (v{})",
                node.node_code, node.url, node.municipality, node.version
            );
        }
        if found.is_empty() {
            println!("  no accounting server on this network");
        }
    }

    #[test]
    fn the_browse_window_is_short_enough_to_wait_through() {
        // A person is watching a progress message while this runs.
        assert!(BROWSE_FOR <= Duration::from_secs(5));
    }
}
