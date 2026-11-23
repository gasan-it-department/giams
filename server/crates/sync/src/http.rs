//! The one way this server makes outgoing requests (cloud uploads, licence
//! renewals).
//!
//! GIAMS's own certificate (`giams-node make-cert`) is self-signed, so the
//! public roots bundled with the HTTP client don't vouch for it. When the
//! cloud or the licensing server runs on GIAMS's certificate, name it in
//! `GIAMS_TLS_CA` and requests to it are trusted; nothing else is relaxed.

use std::time::Duration;

pub fn client(timeout: Duration) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder().timeout(timeout);

    if let Some(path) = std::env::var("GIAMS_TLS_CA").ok().filter(|p| !p.trim().is_empty()) {
        let pem = std::fs::read(path.trim())
            .map_err(|e| anyhow::anyhow!("could not read the certificate named in GIAMS_TLS_CA ({path}): {e}"))?;
        let certificate = reqwest::Certificate::from_pem(&pem)
            .map_err(|e| anyhow::anyhow!("GIAMS_TLS_CA ({path}) is not a PEM certificate: {e}"))?;
        builder = builder.add_root_certificate(certificate);
    }

    Ok(builder.build()?)
}
