//! Announcing this node on the office network.
//!
//! A clerk should never be asked for an IP address. The node advertises itself
//! over mDNS as `_giams._tcp.local`, and the desktop client browses for it — so
//! a fresh workstation finds the accounting server by itself, and the server
//! moving to a different machine does not mean visiting every desk.
//!
//! This is also why the offline design works the way it does: discovery runs on
//! the LAN and needs no internet at all, so an office with its router up but its
//! line down behaves exactly as normal.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use anyhow::Context;
use mdns_sd::{ServiceDaemon, ServiceInfo};

/// The service type workstations browse for.
pub const SERVICE_TYPE: &str = "_giams._tcp.local.";

/// Advertises this node until the returned daemon is dropped.
///
/// Returns the daemon so the caller keeps it alive for the life of the process;
/// dropping it withdraws the announcement, which is what should happen when the
/// server stops.
pub fn announce(
    node_code: &str,
    municipality: &str,
    tenant_id: uuid::Uuid,
    bind: SocketAddr,
) -> anyhow::Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new().context("could not start the network announcement")?;

    let address = advertised_address(bind)?;

    // TXT records let a client tell municipalities apart before connecting —
    // relevant once more than one LGU is running this on a shared network, and
    // harmless before then.
    let mut properties = HashMap::new();
    properties.insert("node".to_string(), node_code.to_string());
    properties.insert("municipality".to_string(), municipality.to_string());
    properties.insert("tenant".to_string(), tenant_id.to_string());
    properties.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());

    let hostname = format!("{}.local.", sanitise(node_code));

    let service = ServiceInfo::new(
        SERVICE_TYPE,
        node_code,
        &hostname,
        &address.to_string()[..],
        bind.port(),
        properties,
    )
    .context("could not describe this server to the network")?;

    daemon
        .register(service)
        .context("could not announce this server on the network")?;

    tracing::info!(
        %address,
        port = bind.port(),
        "announced on the office network as {SERVICE_TYPE}"
    );

    Ok(daemon)
}

/// The address to advertise.
///
/// A node bound to `0.0.0.0` — or to loopback during development — must not
/// advertise that, because a workstation cannot reach it. This finds the
/// address the machine actually uses to reach the LAN, without sending
/// anything: connecting a UDP socket only sets its destination, which is
/// enough for the OS to choose the outbound interface.
fn advertised_address(bind: SocketAddr) -> anyhow::Result<Ipv4Addr> {
    if let IpAddr::V4(ip) = bind.ip() {
        if !ip.is_unspecified() && !ip.is_loopback() {
            return Ok(ip);
        }
    }

    let socket = UdpSocket::bind("0.0.0.0:0").context("could not inspect the network")?;
    // Any routable address works; nothing is transmitted.
    socket
        .connect("192.168.1.1:9")
        .or_else(|_| socket.connect("8.8.8.8:53"))
        .context("could not work out this machine's network address")?;

    match socket.local_addr().context("could not read the network address")?.ip() {
        IpAddr::V4(ip) => Ok(ip),
        IpAddr::V6(_) => Ok(Ipv4Addr::LOCALHOST),
    }
}

/// mDNS hostnames allow letters, digits, and hyphens.
fn sanitise(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();

    if cleaned.is_empty() { "giams-node".to_string() } else { cleaned.to_lowercase() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bound_lan_address_is_advertised_as_is() {
        let bind: SocketAddr = "192.168.1.50:8787".parse().unwrap();
        assert_eq!(advertised_address(bind).unwrap(), Ipv4Addr::new(192, 168, 1, 50));
    }

    #[test]
    fn loopback_and_wildcard_are_replaced_with_a_reachable_address() {
        // Advertising 127.0.0.1 or 0.0.0.0 would tell every workstation to
        // connect to itself.
        for bind in ["127.0.0.1:8787", "0.0.0.0:8787"] {
            let resolved = advertised_address(bind.parse().unwrap()).unwrap();
            assert!(
                !resolved.is_unspecified(),
                "{bind} resolved to an unusable address"
            );
        }
    }

    #[test]
    fn hostnames_are_reduced_to_characters_mdns_accepts() {
        assert_eq!(sanitise("ACCTG-01"), "acctg-01");
        assert_eq!(sanitise("Accounting Office #2"), "accounting-office--2");
        assert_eq!(sanitise(""), "giams-node");
    }
}
