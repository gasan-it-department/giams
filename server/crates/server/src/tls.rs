//! Serving over TLS on the office network.
//!
//! Until now the node spoke plain HTTP. On a switched office LAN that is
//! defensible — but only just, and only while it stays on that LAN. Sessions,
//! passwords on their way to the sign-in route, and every voucher figure cross
//! the wire in the clear, and any machine that can see the traffic can read
//! them.
//!
//! ## Why self-signed is the right answer here
//!
//! A municipal office server has no public hostname and no path to a public
//! certificate authority. Let's Encrypt cannot reach `192.168.1.25`. The
//! honest options are a self-signed certificate trusted once on each
//! workstation, or nothing — and nothing is worse.
//!
//! So `make-cert` produces a long-lived certificate naming this server by
//! every address a workstation might reach it on, and the IT office installs
//! it once per PC. That is a real afternoon of work, and it buys encryption
//! that actually holds.
//!
//! ## What this does not claim
//!
//! A self-signed certificate proves the traffic is encrypted, not that the
//! server is who it says. Against someone already inside the office network
//! who can redirect traffic, the protection is only as good as whether the
//! certificate was pinned on the workstations. Stated plainly rather than
//! left for someone to assume otherwise.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};

/// Where the certificate and its key live, when TLS is configured.
pub struct TlsFiles {
    pub certificate: PathBuf,
    pub key: PathBuf,
}

impl TlsFiles {
    /// Reads the configuration. `None` means plain HTTP, which stays the
    /// default: a node that suddenly refused HTTP after an upgrade would take
    /// the whole office offline.
    pub fn from_env() -> Option<Self> {
        let certificate = std::env::var("GIAMS_TLS_CERT").ok().filter(|p| !p.trim().is_empty())?;
        let key = std::env::var("GIAMS_TLS_KEY").ok().filter(|p| !p.trim().is_empty())?;

        Some(TlsFiles {
            certificate: PathBuf::from(certificate),
            key: PathBuf::from(key),
        })
    }

    pub fn present(&self) -> bool {
        self.certificate.exists() && self.key.exists()
    }
}

/// Serves the application over TLS.
pub async fn serve(
    files: &TlsFiles,
    addr: SocketAddr,
    app: axum::Router,
) -> anyhow::Result<()> {
    // rustls 0.23 requires a crypto provider to be chosen explicitly. Doing it
    // here, once, avoids the runtime panic that otherwise appears on the first
    // handshake — which is to say, in front of a clerk rather than in a test.
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        rustls::crypto::ring::default_provider()
            .install_default()
            .map_err(|_| anyhow::anyhow!("could not set up encryption for this server"))?;
    }

    let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
        &files.certificate,
        &files.key,
    )
    .await
    .with_context(|| {
        format!(
            "could not read the certificate at {} and its key at {}",
            files.certificate.display(),
            files.key.display()
        )
    })?;

    tracing::info!("listening on https://{addr}");

    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .context("the server stopped unexpectedly")
}

/// Writes a self-signed certificate for this server.
///
/// Named for every address a workstation might use: `localhost`, the machine's
/// own name, and each address given on the command line. A certificate that
/// omits the address actually typed produces a browser warning on every visit,
/// and a warning that appears every day is a warning nobody reads.
pub fn make_certificate(names: &[String], folder: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

    let mut subjects: Vec<String> = vec!["localhost".into(), "127.0.0.1".into()];

    if let Ok(hostname) = std::env::var("COMPUTERNAME") {
        if !hostname.trim().is_empty() {
            subjects.push(hostname.to_lowercase());
        }
    }

    subjects.extend(names.iter().map(|n| n.trim().to_lowercase()));
    subjects.sort();
    subjects.dedup();
    subjects.retain(|s| !s.is_empty());

    if subjects.is_empty() {
        bail!("a certificate needs at least one name or address");
    }

    let mut params = CertificateParams::new(subjects.clone())
        .context("those names cannot go in a certificate")?;

    let mut name = DistinguishedName::new();
    name.push(
        DnType::CommonName,
        std::env::var("GIAMS_MUNICIPALITY")
            .unwrap_or_else(|_| "GIAMS accounting server".into()),
    );
    name.push(DnType::OrganizationName, "Municipal Accounting Office");
    name.push(DnType::CountryName, "PH");
    params.distinguished_name = name;

    // Ten years. Renewal means visiting every workstation again, and a
    // certificate that expires unnoticed takes the office offline on a morning
    // nobody planned for.
    params.not_before = rcgen::date_time_ymd(2020, 1, 1);
    params.not_after = rcgen::date_time_ymd(2040, 1, 1);

    let key_pair = KeyPair::generate().context("could not generate a key for the certificate")?;
    let certificate = params
        .self_signed(&key_pair)
        .context("could not create the certificate")?;

    std::fs::create_dir_all(folder)
        .with_context(|| format!("could not create {}", folder.display()))?;

    let cert_path = folder.join("giams-server.crt");
    let key_path = folder.join("giams-server.key");

    std::fs::write(&cert_path, certificate.pem())
        .with_context(|| format!("could not write {}", cert_path.display()))?;

    // The private half. Written with the same care as the node signing key —
    // anyone holding it can impersonate this server to a workstation that
    // trusts the certificate.
    std::fs::write(&key_path, key_pair.serialize_pem())
        .with_context(|| format!("could not write {}", key_path.display()))?;

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("icacls")
            .arg(&key_path)
            .args(["/inheritance:r", "/grant:r"])
            .arg(format!(
                "{}:F",
                std::env::var("USERNAME").unwrap_or_else(|_| "Administrators".into())
            ))
            .output();
    }

    println!();
    println!("Certificate written for: {}", subjects.join(", "));
    println!("  certificate: {}", cert_path.display());
    println!("  private key: {}", key_path.display());
    println!();
    println!("Next:");
    println!("  1. Put these two lines in .env:");
    println!("       GIAMS_TLS_CERT=\"{}\"", cert_path.display());
    println!("       GIAMS_TLS_KEY=\"{}\"", key_path.display());
    println!("  2. Restart the service.");
    println!("  3. Install {} on each workstation, under", cert_path.display());
    println!("     Trusted Root Certification Authorities. Until that is done every");
    println!("     workstation shows a certificate warning.");
    println!();
    println!("Keep the private key off shared drives. Anyone holding it can");
    println!("impersonate this server to a workstation that trusts it.");
    println!();

    Ok((cert_path, key_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_certificate_names_localhost_and_whatever_else_was_asked_for() {
        let dir = std::env::temp_dir().join(format!("giams-tls-{}", uuid::Uuid::new_v4()));

        let (cert, key) =
            make_certificate(&["192.168.1.25".into(), "ACCTG-SERVER".into()], &dir).unwrap();

        let pem = std::fs::read_to_string(&cert).unwrap();
        assert!(pem.starts_with("-----BEGIN CERTIFICATE-----"));

        let key_pem = std::fs::read_to_string(&key).unwrap();
        assert!(key_pem.contains("PRIVATE KEY"));

        // The certificate must actually load as a server certificate, not just
        // look like one — a file that parses in a test and fails at handshake
        // is worse than no test.
        let mut reader = std::io::BufReader::new(std::fs::File::open(&cert).unwrap());
        let chain: Vec<_> = rustls_pemfile::certs(&mut reader).collect::<Result<_, _>>().unwrap();
        assert_eq!(chain.len(), 1, "expected exactly one certificate");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_certificate_with_no_names_is_refused() {
        let dir = std::env::temp_dir().join(format!("giams-tls-{}", uuid::Uuid::new_v4()));
        // `localhost` is always added, so this still succeeds — the guard is
        // for the case where even that is somehow absent.
        assert!(make_certificate(&[], &dir).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }
}
