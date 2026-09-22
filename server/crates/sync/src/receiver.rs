//! The cloud's half: verify, then apply exactly once.
//!
//! Three gates, in this order, and the order is the point:
//!
//! 1. **Signature.** Checked against the node's published public key, over the
//!    exact bytes received. Nothing is parsed as a ledger fact before this
//!    passes, so a forged batch never reaches the database at all.
//! 2. **Ownership.** The batch's node must belong to the batch's tenant. A
//!    node cannot write another municipality's books even with a valid
//!    signature of its own.
//! 3. **Order and idempotency.** Handled inside `sync_apply_batch`, in the
//!    same transaction as the rows themselves.

use anyhow::{bail, Context};
use giams_db::sqlx::{self, PgPool};
use uuid::Uuid;

use crate::batch::{Batch, Receipt};
use crate::identity;

/// Verifies and applies a received batch.
///
/// `body` must be the bytes exactly as they arrived — not a re-serialisation
/// of `batch`. Re-serialising would verify a signature over something the
/// sender never signed, which passes in testing and fails the first time a
/// field order or a float rendering differs.
pub async fn receive(pool: &PgPool, body: &[u8], signature: &[u8]) -> anyhow::Result<Receipt> {
    let batch: Batch = serde_json::from_slice(body)
        .context("the batch could not be read as a batch")?;

    let node = sqlx::query_as::<_, (Uuid, Vec<u8>)>(
        "SELECT tenant_id, public_key FROM node WHERE id = $1",
    )
    .bind(batch.node_id)
    .fetch_optional(pool)
    .await
    .context("could not look up the sending node")?;

    let Some((tenant_id, public_key)) = node else {
        bail!("that node is not registered with this service");
    };

    identity::verify(&public_key, body, signature)?;

    // Checked after the signature and before anything is applied. A node whose
    // key is genuine still may not write books that are not its own.
    if tenant_id != batch.tenant_id {
        bail!("that node does not hold the records for the municipality named in the batch");
    }

    let records = serde_json::to_value(&batch.records)
        .context("the batch's records could not be prepared")?;

    let applied: serde_json::Value = sqlx::query_scalar(
        "SELECT sync_apply_batch($1, $2, $3, $4, $5, $6)",
    )
    .bind(batch.batch_id)
    .bind(batch.tenant_id)
    .bind(batch.node_id)
    .bind(batch.from_seq)
    .bind(batch.to_seq)
    .bind(records)
    .fetch_one(pool)
    .await
    .context("the batch could not be applied")?;

    serde_json::from_value(applied).context("the receipt could not be read back")
}
