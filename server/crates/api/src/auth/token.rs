//! Tokens.
//!
//! Two quite different things, deliberately not interchangeable:
//!
//! * **Access token** — a short-lived signed JWT carrying the claims a request
//!   needs. Fifteen minutes, held in memory by the client and never written to
//!   disk. It is not checked against the database on every request, which is
//!   what keeps request handling cheap; the short lifetime is what bounds the
//!   damage of that.
//!
//! * **Refresh token** — an opaque 256-bit random value. It has no structure
//!   and carries no claims, so nothing can be learned from it. Only its
//!   SHA-256 is stored, so a stolen database backup yields no usable sessions.
//!   Each use rotates it; replaying a retired one is treated as theft.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const ACCESS_TOKEN_MINUTES: i64 = 15;
pub const REFRESH_TOKEN_HOURS: i64 = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the user id.
    pub sub: Uuid,
    /// The municipality this session belongs to.
    pub tid: Uuid,
    /// The session this token was issued under, so it can be traced and revoked.
    pub sid: Uuid,
    /// Permission codes. Embedded so ordinary requests need no database round
    /// trip; bounded by the fifteen-minute lifetime, after which a revoked
    /// permission stops working.
    pub perms: Vec<String>,
    pub username: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Clone)]
pub struct TokenKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl TokenKeys {
    /// Builds the signing keys from the configured secret.
    ///
    /// Refuses anything under 32 bytes. A weak signing secret makes every
    /// other control in the system decorative, so this is a hard failure at
    /// startup rather than a warning nobody reads.
    pub fn from_secret(secret: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            secret.len() >= 32,
            "GIAMS_TOKEN_SECRET must be at least 32 characters; \
             generate one with `openssl rand -base64 48`"
        );

        Ok(TokenKeys {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
        })
    }

    pub fn issue_access_token(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
        session_id: Uuid,
        username: &str,
        perms: Vec<String>,
    ) -> anyhow::Result<(String, DateTime<Utc>)> {
        let now = Utc::now();
        let expires_at = now + Duration::minutes(ACCESS_TOKEN_MINUTES);

        let claims = Claims {
            sub: user_id,
            tid: tenant_id,
            sid: session_id,
            perms,
            username: username.to_string(),
            exp: expires_at.timestamp(),
            iat: now.timestamp(),
        };

        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)?;
        Ok((token, expires_at))
    }

    pub fn verify_access_token(&self, token: &str) -> Option<Claims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 5; // tolerate small clock skew between workstations
        decode::<Claims>(token, &self.decoding, &validation)
            .ok()
            .map(|data| data.claims)
    }
}

/// A freshly minted refresh token: the value to hand to the client, and the
/// hash to keep.
pub struct RefreshToken {
    pub value: String,
    pub hash: Vec<u8>,
    pub expires_at: DateTime<Utc>,
}

pub fn new_refresh_token() -> RefreshToken {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);

    let value = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_refresh_token(&value);

    RefreshToken {
        value,
        hash,
        expires_at: Utc::now() + Duration::hours(REFRESH_TOKEN_HOURS),
    }
}

/// SHA-256, not Argon2.
///
/// Deliberate: this input is 256 bits of machine-generated randomness, not a
/// human-chosen password. There is no dictionary to attack, so the slow hash
/// would buy nothing and would put an Argon2 computation on the hot path of
/// every token refresh.
pub fn hash_refresh_token(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> TokenKeys {
        TokenKeys::from_secret("a-test-secret-that-is-long-enough-to-pass").unwrap()
    }

    #[test]
    fn a_short_secret_is_refused_at_startup() {
        assert!(TokenKeys::from_secret("too-short").is_err());
    }

    #[test]
    fn an_issued_token_verifies_and_carries_its_claims() {
        let (user, tenant, session) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let (token, _) = keys()
            .issue_access_token(user, tenant, session, "jribleza", vec!["jev.post".into()])
            .unwrap();

        let claims = keys().verify_access_token(&token).expect("should verify");
        assert_eq!(claims.sub, user);
        assert_eq!(claims.tid, tenant);
        assert_eq!(claims.sid, session);
        assert_eq!(claims.perms, vec!["jev.post".to_string()]);
    }

    #[test]
    fn a_token_signed_with_another_secret_is_rejected() {
        let (token, _) = TokenKeys::from_secret("a-completely-different-secret-value-here")
            .unwrap()
            .issue_access_token(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), "x", vec![])
            .unwrap();

        assert!(keys().verify_access_token(&token).is_none());
    }

    #[test]
    fn a_tampered_token_is_rejected() {
        let (token, _) = keys()
            .issue_access_token(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), "clerk", vec![])
            .unwrap();

        // Flip a character in the payload segment.
        let mut parts: Vec<&str> = token.split('.').collect();
        let payload = parts[1].to_string();
        let tampered = format!("{}X{}", &payload[..payload.len() - 1], "");
        parts[1] = &tampered;

        assert!(keys().verify_access_token(&parts.join(".")).is_none());
    }

    #[test]
    fn refresh_tokens_are_unpredictable_and_stored_only_as_hashes() {
        let a = new_refresh_token();
        let b = new_refresh_token();

        assert_ne!(a.value, b.value);
        assert_ne!(a.hash, b.hash);
        assert_eq!(a.hash.len(), 32);
        // The stored form must not contain the token itself.
        assert!(!String::from_utf8_lossy(&a.hash).contains(&a.value));
        assert_eq!(hash_refresh_token(&a.value), a.hash);
    }
}
