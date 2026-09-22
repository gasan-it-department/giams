//! The node's signing key.
//!
//! Every batch a node ships is signed with an Ed25519 private key that belongs
//! to that node alone. The cloud verifies it against `node.public_key` before
//! applying anything.
//!
//! This is what makes a stolen API credential insufficient to forge ledger
//! entries. An attacker who obtains the cloud's authentication token can talk
//! to the receiver, but cannot produce a batch it will accept — the signature
//! will not verify, and the batch is rejected before a single row is applied.
//!
//! ## Where the private key lives
//!
//! In a file, not in the database. The threat this defends against includes a
//! stolen database dump, and a key stored in the database it protects defends
//! against nothing. The file is created with owner-only permissions and read
//! once at startup.
//!
//! Losing it is recoverable: generate a new one and re-publish the public half.
//! Batches already acknowledged stay acknowledged; only unshipped ones are
//! re-signed. Losing it is not a data loss event, which is why a file is
//! proportionate here.

use std::path::{Path, PathBuf};

use anyhow::Context;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use giams_db::sqlx::PgPool;
use uuid::Uuid;

/// A node's Ed25519 key pair.
pub struct NodeKey {
    signing: SigningKey,
}

impl std::fmt::Debug for NodeKey {
    /// Never prints the private half. A key that reaches a log file is a key
    /// that has to be rotated.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeKey")
            .field("public_key", &hex(self.public_key().as_bytes()))
            .finish()
    }
}

impl NodeKey {
    /// Loads the key at `path`, creating one if there is none.
    ///
    /// Creation is the first-run case and is logged loudly, because a key that
    /// appears unexpectedly on an existing installation means the old one was
    /// lost or the path changed — and every batch signed with the previous key
    /// will now be rejected until the public half is re-published.
    pub fn load_or_create(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let bytes = std::fs::read(path)
                .with_context(|| format!("could not read the node key at {}", path.display()))?;

            let raw: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
                anyhow::anyhow!(
                    "the node key at {} is {} bytes; an Ed25519 key is 32",
                    path.display(),
                    bytes.len()
                )
            })?;

            return Ok(NodeKey { signing: SigningKey::from_bytes(&raw) });
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("could not create the directory for {}", path.display())
            })?;
        }

        let signing = SigningKey::generate(&mut rand::rngs::OsRng);
        write_private(path, &signing.to_bytes())?;

        tracing::warn!(
            path = %path.display(),
            "generated a new signing key for this node -- its public half must be \
             published to the cloud before any batch will be accepted"
        );

        Ok(NodeKey { signing })
    }

    pub fn public_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    /// Signs the exact bytes that will go on the wire.
    ///
    /// Deliberately over the serialised body rather than over a re-derived
    /// canonical form: the receiver verifies the bytes it actually received,
    /// so there is no canonicalisation step for the two sides to disagree
    /// about — the classic way a signature scheme is defeated in practice.
    pub fn sign(&self, body: &[u8]) -> Signature {
        self.signing.sign(body)
    }

    /// Records this node's public key so the cloud can verify its batches.
    pub async fn publish(&self, pool: &PgPool, node_id: Uuid) -> anyhow::Result<()> {
        let public = self.public_key().to_bytes();

        giams_db::sqlx::query("UPDATE node SET public_key = $2 WHERE id = $1")
            .bind(node_id)
            .bind(public.as_slice())
            .execute(pool)
            .await
            .context("could not publish this node's public key")?;

        Ok(())
    }

    /// Where the key lives by default: beside the database configuration,
    /// overridable for a node that keeps its secrets elsewhere.
    pub fn default_path() -> PathBuf {
        std::env::var("GIAMS_NODE_KEY_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("node-signing.key"))
    }
}

/// Verifies a batch signature against a node's published public key.
pub fn verify(public_key: &[u8], body: &[u8], signature: &[u8]) -> anyhow::Result<()> {
    let key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| anyhow::anyhow!("that node has no usable public key on record"))?;

    let key = VerifyingKey::from_bytes(&key)
        .context("that node's public key on record is not a valid Ed25519 key")?;

    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| anyhow::anyhow!("the batch signature is the wrong length"))?;

    key.verify(body, &Signature::from_bytes(&signature))
        .map_err(|_| anyhow::anyhow!("the batch signature does not match its contents"))
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::io::Write;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("could not create the node key at {}", path.display()))?;

    file.write_all(bytes).context("could not write the node key")?;
    Ok(())
}

#[cfg(windows)]
fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    // `create_new` rather than `create`: if something already exists here,
    // overwriting it would destroy a key still in use by an existing node.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("could not create the node key at {}", path.display()))?;

    file.write_all(bytes).context("could not write the node key")?;
    drop(file);

    // Windows has no mode bits. The service account's own directory is the
    // protection; this only removes the inherited "Users can read" grant where
    // icacls is available, and is not fatal if it is not.
    let _ = std::process::Command::new("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r"])
        .arg(format!("{}:F", whoami()))
        .output();

    Ok(())
}

#[cfg(windows)]
fn whoami() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "Administrators".into())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(not(any(unix, windows)))]
fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    std::fs::write(path, bytes)
        .with_context(|| format!("could not create the node key at {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_verifies_against_its_own_key_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("giams-key-{}", Uuid::new_v4()));
        let path = dir.join("node.key");

        let key = NodeKey::load_or_create(&path).unwrap();
        let body = br#"{"batch_id":"x","records":[]}"#;
        let signature = key.sign(body);

        verify(key.public_key().as_bytes(), body, &signature.to_bytes())
            .expect("a batch must verify against the key that signed it");

        // A single altered byte must fail. This is the property the whole
        // scheme rests on.
        let tampered = br#"{"batch_id":"y","records":[]}"#;
        assert!(
            verify(key.public_key().as_bytes(), tampered, &signature.to_bytes()).is_err(),
            "an altered batch was accepted"
        );

        let other = NodeKey::load_or_create(&dir.join("other.key")).unwrap();
        assert!(
            verify(other.public_key().as_bytes(), body, &signature.to_bytes()).is_err(),
            "a batch verified against the wrong node's key"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_key_is_loaded_back_rather_than_regenerated() {
        let dir = std::env::temp_dir().join(format!("giams-key-{}", Uuid::new_v4()));
        let path = dir.join("node.key");

        let first = NodeKey::load_or_create(&path).unwrap();
        let second = NodeKey::load_or_create(&path).unwrap();

        assert_eq!(
            first.public_key().to_bytes(),
            second.public_key().to_bytes(),
            "restarting the node changed its identity"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_private_half_never_appears_in_debug_output() {
        let dir = std::env::temp_dir().join(format!("giams-key-{}", Uuid::new_v4()));
        let key = NodeKey::load_or_create(&dir.join("node.key")).unwrap();

        let printed = format!("{key:?}");
        let secret = hex(&key.signing.to_bytes());

        assert!(!printed.contains(&secret), "the signing key was printed");
        assert!(printed.contains(&hex(key.public_key().as_bytes())));

        std::fs::remove_dir_all(&dir).ok();
    }
}
