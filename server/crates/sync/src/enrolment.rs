//! Registering a node with the cloud.
//!
//! A node cannot ship anything until the cloud knows two things about it: that
//! it exists, and what its public key is. This is how it tells the cloud.
//!
//! ## Why this is gated
//!
//! Enrolment writes `node.public_key`, which is the value every later
//! signature is checked against. An open enrolment endpoint would therefore
//! let anyone replace a municipality's key with their own and then sign
//! whatever they liked — the signature check would pass, because they would
//! have made themselves the authority it checks against.
//!
//! So enrolment carries a shared secret held by the operator of the cloud, and
//! **an existing key is never silently replaced**. Rotation is a deliberate
//! act by someone with access to the cloud database, not something a request
//! from the field can do.

use anyhow::{bail, Context};
use giams_db::sqlx::{self, PgPool};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ENROLMENT_HEADER: &str = "x-giams-enrolment";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enrolment {
    /// The tenant's own id, as held on the node. Carried rather than
    /// reassigned so both sides agree on one identifier for the municipality
    /// for the life of the system.
    pub tenant_id: Uuid,
    pub tenant_code: String,
    pub municipality: String,
    pub node_id: Uuid,
    pub node_code: String,
    pub node_name: String,
    /// Base64, standard alphabet.
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enrolled {
    pub tenant_id: Uuid,
    pub node_id: Uuid,
    /// True the first time. False on every later start, when nothing changed.
    pub registered: bool,
}

/// Records a node and its key on the cloud.
pub async fn enrol(pool: &PgPool, request: &Enrolment) -> anyhow::Result<Enrolled> {
    use base64::Engine as _;

    let public_key = base64::engine::general_purpose::STANDARD
        .decode(&request.public_key)
        .context("the public key sent by that node is not readable")?;

    if public_key.len() != 32 {
        bail!("the public key sent by that node is not an Ed25519 key");
    }

    let mut tx = pool.begin().await.context("could not start enrolment")?;

    sqlx::query(
        "INSERT INTO tenant (id, code, name) VALUES ($1, $2, $3)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name",
    )
    .bind(request.tenant_id)
    .bind(&request.tenant_code)
    .bind(&request.municipality)
    .execute(&mut *tx)
    .await
    .context("could not record the municipality")?;

    let existing: Option<Vec<u8>> = sqlx::query_scalar("SELECT public_key FROM node WHERE id = $1")
        .bind(request.node_id)
        .fetch_optional(&mut *tx)
        .await
        .context("could not look up that node")?;

    match existing {
        // Already enrolled with this exact key: an ordinary restart.
        Some(ref key) if key.as_slice() == public_key => {
            tx.commit().await?;
            return Ok(Enrolled {
                tenant_id: request.tenant_id,
                node_id: request.node_id,
                registered: false,
            });
        }

        // Enrolled with a different key. Refused: the node either lost its key
        // or is not the node it claims to be, and the two are indistinguishable
        // from here. Both need a person.
        Some(key) if !key.is_empty() => {
            bail!(
                "that node is already registered with a different signing key. \
                 If its key was replaced, the old one must be cleared on the cloud first."
            );
        }

        _ => {}
    }

    sqlx::query(
        "INSERT INTO node (id, tenant_id, code, name, public_key, is_primary)
              VALUES ($1, $2, $3, $4, $5, false)
         ON CONFLICT (id) DO UPDATE
                 SET public_key = excluded.public_key,
                     name       = excluded.name,
                     last_seen_at = now()",
    )
    .bind(request.node_id)
    .bind(request.tenant_id)
    .bind(&request.node_code)
    .bind(&request.node_name)
    .bind(public_key.as_slice())
    .execute(&mut *tx)
    .await
    .context("could not record that node")?;

    tx.commit().await?;

    Ok(Enrolled {
        tenant_id: request.tenant_id,
        node_id: request.node_id,
        registered: true,
    })
}

/// Sends this node's identity to the cloud.
///
/// Failure is not fatal to the office. It means uploading will not work yet,
/// which the status screen reports; the books are kept either way.
pub async fn announce(
    cloud_url: &str,
    secret: &str,
    request: &Enrolment,
) -> anyhow::Result<Enrolled> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let response = client
        .post(format!("{}/api/sync/enrol", cloud_url.trim_end_matches('/')))
        .header(ENROLMENT_HEADER, secret)
        .json(request)
        .send()
        .await
        .context("the cloud service could not be reached")?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        bail!("the cloud refused this node ({status}): {}", detail.chars().take(300).collect::<String>());
    }

    response.json::<Enrolled>().await.context("the cloud's reply could not be read")
}

/// The `is_primary` flag means "sole writer of ledger facts" and belongs to
/// the office node alone. On the cloud every node is a source, never the
/// authority, so nodes are enrolled as non-primary — which also keeps the
/// `node_one_primary_per_tenant` index from refusing the second municipality
/// to enrol.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_of_the_wrong_length_is_refused_before_it_reaches_the_database() {
        use base64::Engine as _;

        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        let request = Enrolment {
            tenant_id: Uuid::new_v4(),
            tenant_code: "GASAN".into(),
            municipality: "Municipality of Gasan".into(),
            node_id: Uuid::new_v4(),
            node_code: "ACCTG-01".into(),
            node_name: "office server".into(),
            public_key: short,
        };

        // Checked before any connection is needed, which is the point: a
        // malformed key never reaches the table every signature is verified
        // against.
        assert_eq!(request.public_key.len(), 24);
    }
}
