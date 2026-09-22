//! Binding a licence to one machine.
//!
//! A licence file copied to another municipality's server should not work
//! there. The token can carry a fingerprint of the machine it was issued for,
//! and the running system checks it.
//!
//! ## What it is made of
//!
//! Identifiers that survive an ordinary reinstall of the application but not a
//! move to different hardware. On Windows that is the machine GUID the OS
//! generates at install, plus the system drive's volume serial.
//!
//! ## What it is not
//!
//! Not an anti-piracy measure that a determined person cannot defeat. Anyone
//! with administrator access to a machine can change what these read. It stops
//! the realistic case — a licence file emailed to the next municipality —
//! and does not pretend to stop more than that.
//!
//! An unbound licence has no fingerprint at all, and that is the ordinary
//! shape for a municipality running its own installation.

use sha2::{Digest, Sha256};

/// A stable identifier for this machine, or `None` where one cannot be read.
///
/// A machine that cannot be fingerprinted is not punished for it: a licence
/// issued without a fingerprint runs anywhere, and one issued *with* a
/// fingerprint refuses to run where none can be read — which is the safe way
/// round.
pub fn machine() -> Option<String> {
    let parts = read_identifiers();
    if parts.is_empty() {
        return None;
    }

    let mut hasher = Sha256::new();
    for part in &parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }

    // Hashed, so the licence file never carries the raw machine identifiers of
    // the office it was issued to.
    Some(hex(&hasher.finalize()[..16]))
}

/// Whether a licence's fingerprint matches this machine.
pub fn matches(expected: Option<&str>) -> bool {
    match expected {
        // Not bound to any machine.
        None => true,
        Some(expected) => machine().is_some_and(|actual| actual == expected),
    }
}

#[cfg(windows)]
fn read_identifiers() -> Vec<String> {
    let mut parts = Vec::new();

    // The GUID Windows generates when it is installed. Survives application
    // reinstalls; does not survive moving to another machine.
    if let Some(guid) = registry_value(
        r"HKLM\SOFTWARE\Microsoft\Cryptography",
        "MachineGuid",
    ) {
        parts.push(guid);
    }

    if let Some(uuid) = wmic("csproduct", "uuid") {
        parts.push(uuid);
    }

    parts
}

#[cfg(windows)]
fn registry_value(path: &str, name: &str) -> Option<String> {
    let output = std::process::Command::new("reg")
        .args(["query", path, "/v", name])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find(|l| l.contains(name))
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_string)
}

#[cfg(windows)]
fn wmic(class: &str, field: &str) -> Option<String> {
    let output = std::process::Command::new("wmic")
        .args([class, "get", field])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.eq_ignore_ascii_case(field))
        .map(str::to_string)
}

#[cfg(not(windows))]
fn read_identifiers() -> Vec<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unbound_licence_runs_anywhere() {
        assert!(matches(None));
    }

    #[test]
    fn a_licence_for_another_machine_does_not_match_this_one() {
        assert!(!matches(Some("0000000000000000000000000000000")));
    }

    #[test]
    fn the_fingerprint_is_stable_within_a_run() {
        assert_eq!(machine(), machine());
    }
}
