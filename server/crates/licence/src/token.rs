//! The licence token: what it contains, and how it is signed and checked.
//!
//! A licence is a short signed string. It verifies with no internet at all,
//! because everything needed to check it is in the token and the public half
//! of the signing key is compiled into the binary.
//!
//! ```text
//! GIAMS1.<base64url payload>.<base64url signature>
//! ```
//!
//! ## Why not a JWT
//!
//! A JWT carries its own algorithm in its header, and the reader is expected
//! to trust it. That single design decision is the source of the entire
//! `alg: none` and RS256/HS256 confusion family of vulnerabilities. This
//! format has one algorithm, named in the prefix, and a verifier that knows it
//! before it reads anything. There is no field an attacker can set to change
//! how their own token is checked.
//!
//! ## What is signed
//!
//! The payload bytes exactly as they appear between the dots — not a
//! re-serialisation. Two implementations that both "produce the same JSON"
//! eventually will not, and the day they differ is the day every licence in
//! the field stops verifying.

use anyhow::{bail, Context};
use base64::Engine as _;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const PREFIX: &str = "GIAMS1";

fn b64() -> base64::engine::general_purpose::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

/// What a licence asserts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Claims {
    /// The municipality this licence is for.
    pub tenant_id: Uuid,
    /// Shown to the office, so a licence is identifiable without decoding it.
    pub municipality: String,
    pub plan: String,
    /// Workstations permitted. Counted by the node, not by each workstation.
    pub seats: u32,
    pub issued_at: DateTime<Utc>,
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Days of full function after expiry, before read-only.
    pub grace_days: i64,
    /// Binds the licence to one machine. Absent means it is not bound, which
    /// is what a municipality running on its own hardware gets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
}

/// Signs a set of claims. Held by the licensing authority alone.
pub fn issue(claims: &Claims, signing_key: &SigningKey) -> anyhow::Result<String> {
    let payload = serde_json::to_vec(claims).context("the licence could not be prepared")?;
    let encoded = b64().encode(&payload);
    let signature = signing_key.sign(encoded.as_bytes());

    Ok(format!("{PREFIX}.{encoded}.{}", b64().encode(signature.to_bytes())))
}

/// Checks a token's signature and returns what it claims.
///
/// Says nothing about whether the licence is *current* — that is
/// [`crate::Licence::capability_at`], and it is deliberately a separate step.
/// Conflating "genuine" with "in force" is how an expired licence ends up
/// treated as a forged one, and the two need very different messages.
pub fn verify(token: &str, public_key: &VerifyingKey) -> anyhow::Result<Claims> {
    let mut parts = token.trim().split('.');

    let (Some(prefix), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        bail!("that licence key is not in the expected form");
    };

    if prefix != PREFIX {
        bail!("that licence key was issued for a different version of this system");
    }

    let signature_bytes = b64()
        .decode(signature)
        .map_err(|_| anyhow::anyhow!("that licence key is damaged"))?;

    let signature_bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("that licence key is damaged"))?;

    // Verified over the encoded payload, which is exactly what was signed.
    public_key
        .verify(payload.as_bytes(), &Signature::from_bytes(&signature_bytes))
        .map_err(|_| {
            anyhow::anyhow!("that licence key is not genuine, or has been altered since it was issued")
        })?;

    let decoded = b64()
        .decode(payload)
        .map_err(|_| anyhow::anyhow!("that licence key is damaged"))?;

    serde_json::from_slice(&decoded)
        .map_err(|_| anyhow::anyhow!("that licence key is for a newer version of this system"))
}

/// The public half the running system checks against.
///
/// Compiled in rather than read from a file, so replacing it means replacing
/// the binary — which is a very different act from editing a config file, and
/// one an installation can be checked for.
pub fn authority_public_key() -> anyhow::Result<VerifyingKey> {
    let raw = b64()
        .decode(env!("GIAMS_LICENCE_PUBLIC_KEY"))
        .context("this build has a malformed licensing key")?;

    let raw: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("this build has a malformed licensing key"))?;

    VerifyingKey::from_bytes(&raw).context("this build has a malformed licensing key")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims() -> Claims {
        Claims {
            tenant_id: Uuid::nil(),
            municipality: "Municipality of Gasan".into(),
            plan: "standard".into(),
            seats: 10,
            issued_at: Utc::now(),
            not_before: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::days(365),
            grace_days: 14,
            fingerprint: None,
            features: vec![],
        }
    }

    fn key() -> SigningKey {
        SigningKey::generate(&mut rand::rngs::OsRng)
    }

    #[test]
    fn a_licence_verifies_against_the_authority_that_issued_it() {
        let signing = key();
        let token = issue(&claims(), &signing).unwrap();

        let read = verify(&token, &signing.verifying_key()).unwrap();
        assert_eq!(read.municipality, "Municipality of Gasan");
        assert_eq!(read.seats, 10);
    }

    #[test]
    fn editing_any_field_invalidates_it() {
        let signing = key();
        let token = issue(&claims(), &signing).unwrap();

        // The obvious attack: decode, raise the seat count, re-encode.
        let mut parts: Vec<&str> = token.split('.').collect();
        let decoded = b64().decode(parts[1]).unwrap();
        let mut edited: Claims = serde_json::from_slice(&decoded).unwrap();
        edited.seats = 10_000;
        edited.expires_at = Utc::now() + chrono::Duration::days(36_500);

        let forged = b64().encode(serde_json::to_vec(&edited).unwrap());
        parts[1] = &forged;
        let forged_token = parts.join(".");

        assert!(
            verify(&forged_token, &signing.verifying_key()).is_err(),
            "an edited licence was accepted"
        );
    }

    #[test]
    fn a_licence_from_another_authority_is_refused() {
        let token = issue(&claims(), &key()).unwrap();
        assert!(verify(&token, &key().verifying_key()).is_err());
    }

    #[test]
    fn malformed_input_is_refused_in_words_an_office_can_read() {
        let signing = key();
        for bad in ["", "not-a-licence", "GIAMS1.only-two", "GIAMS2.a.b", "GIAMS1.a.b.c"] {
            let e = verify(bad, &signing.verifying_key()).unwrap_err().to_string();
            assert!(
                !e.contains("base64") && !e.contains("serde") && !e.contains("Error"),
                "the message for {bad:?} is written for a developer: {e}"
            );
        }
    }
}
