//! The node's half: claim, sign, send, acknowledge.
//!
//! Runs on the office server. Uploading is a background activity that must
//! never affect anything a clerk is doing — a failed upload is not a failed
//! voucher, and the two must not be able to be confused. So every failure
//! here is logged and retried later, and none of it is surfaced as an error on
//! a form.
//!
//! ## The ordering that matters
//!
//! Claim, send, *then* acknowledge. If the process dies between sending and
//! acknowledging, the batch is re-sent with the same id and the receiver
//! replays its original answer — nothing is applied twice. Acknowledging
//! before sending would lose records permanently on the same crash, which is
//! why the order is not an implementation detail.

use anyhow::Context;
use giams_db::sqlx::{self, PgPool};
use uuid::Uuid;

use crate::batch::{Batch, Receipt, Record, SIGNATURE_HEADER};
use crate::identity::NodeKey;

/// What one upload attempt did.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Nothing was waiting.
    UpToDate,
    Shipped { batch_id: Uuid, records: i64, through: i64 },
    /// The cloud could not be reached. Ordinary on an unreliable connection,
    /// and not an error worth alarming anybody about.
    Unreachable { reason: String },
    /// The cloud refused the batch. This one *is* worth alarming about: it
    /// means the two sides disagree about the ledger.
    Rejected { batch_id: Uuid, reason: String },
    /// The queue has a hole in it. Uploading stops until somebody looks.
    Halted { reason: String },
}

#[derive(Debug, sqlx::FromRow)]
struct ClaimRow {
    batch_id: Uuid,
    from_seq: i64,
    to_seq: i64,
    record_count: i32,
}

#[derive(Debug, sqlx::FromRow)]
struct OutboxRow {
    node_seq: i64,
    entity: String,
    entity_id: Uuid,
    op: String,
    payload: serde_json::Value,
}

/// Ships one batch, if there is one to ship.
///
/// Deliberately one batch per call rather than a loop: the caller decides the
/// pace, and a node with a year of backlog does not monopolise its own
/// database for an hour on the first connection it gets.
pub async fn ship_once(
    pool: &PgPool,
    node_id: Uuid,
    tenant_id: Uuid,
    cloud_url: &str,
    key: &NodeKey,
    limit: i32,
) -> anyhow::Result<Outcome> {
    // Checked before claiming, so a hole is reported as itself rather than as
    // whatever the claim happens to fail with.
    let gaps = crate::find_gaps(pool, node_id).await?;
    if let Some(gap) = gaps.first() {
        return Ok(Outcome::Halted {
            reason: format!(
                "Records {} to {} are missing from the upload queue.",
                gap.missing_from, gap.missing_to
            ),
        });
    }

    let claim = sqlx::query_as::<_, ClaimRow>(
        "SELECT batch_id, from_seq, to_seq, record_count FROM sync_claim_batch($1, $2)",
    )
    .bind(node_id)
    .bind(limit)
    .fetch_optional(pool)
    .await
    .context("could not prepare a batch to upload")?;

    let Some(claim) = claim else {
        return Ok(Outcome::UpToDate);
    };

    let rows = sqlx::query_as::<_, OutboxRow>(
        "SELECT node_seq, entity, entity_id, op, payload
           FROM sync_outbox
          WHERE batch_id = $1
          ORDER BY node_seq",
    )
    .bind(claim.batch_id)
    .fetch_all(pool)
    .await
    .context("could not read the batch to upload")?;

    let batch = Batch {
        batch_id: claim.batch_id,
        tenant_id,
        node_id,
        from_seq: claim.from_seq,
        to_seq: claim.to_seq,
        records: rows
            .into_iter()
            .map(|r| Record {
                node_seq: r.node_seq,
                entity: r.entity,
                entity_id: r.entity_id,
                op: r.op,
                payload: r.payload,
            })
            .collect(),
    };

    let body = batch.to_bytes()?;
    let signature = key.sign(&body);

    match send(cloud_url, &body, &signature.to_bytes()).await {
        Ok(receipt) => {
            sqlx::query("SELECT sync_acknowledge_batch($1)")
                .bind(claim.batch_id)
                .execute(pool)
                .await
                .context("could not mark the batch as uploaded")?;

            Ok(Outcome::Shipped {
                batch_id: claim.batch_id,
                records: i64::from(claim.record_count),
                through: receipt.through,
            })
        }

        Err(SendError::Unreachable(reason)) => {
            release(pool, claim.batch_id, &reason).await?;
            Ok(Outcome::Unreachable { reason })
        }

        Err(SendError::Refused(reason)) => {
            // Released so the records are not stranded inside a batch that
            // will never be sent. They keep their sequence numbers, so the
            // next attempt sends the same run and the receiver's idempotency
            // check settles whether it was already applied.
            release(pool, claim.batch_id, &reason).await?;
            Ok(Outcome::Rejected { batch_id: claim.batch_id, reason })
        }
    }
}

async fn release(pool: &PgPool, batch_id: Uuid, reason: &str) -> anyhow::Result<()> {
    sqlx::query("SELECT sync_release_batch($1, $2)")
        .bind(batch_id)
        .bind(reason)
        .execute(pool)
        .await
        .context("could not return the batch to the upload queue")?;
    Ok(())
}

enum SendError {
    /// No connection, or the cloud is down. Try again later.
    Unreachable(String),
    /// The cloud answered and said no.
    Refused(String),
}

async fn send(cloud_url: &str, body: &[u8], signature: &[u8]) -> Result<Receipt, SendError> {
    use base64::Engine as _;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| SendError::Unreachable(e.to_string()))?;

    let response = client
        .post(format!("{}/api/sync/batches", cloud_url.trim_end_matches('/')))
        .header("content-type", "application/json")
        .header(
            SIGNATURE_HEADER,
            base64::engine::general_purpose::STANDARD.encode(signature),
        )
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| SendError::Unreachable(e.to_string()))?;

    let status = response.status();

    if status.is_success() {
        return response
            .json::<Receipt>()
            .await
            .map_err(|e| SendError::Refused(format!("the cloud's reply could not be read: {e}")));
    }

    // A 5xx is the cloud having a bad day, not a disagreement about the
    // ledger. Retried quietly; a 4xx is not.
    let detail = response.text().await.unwrap_or_default();
    let detail = detail.chars().take(400).collect::<String>();

    if status.is_server_error() {
        Err(SendError::Unreachable(format!("the cloud returned {status}")))
    } else {
        Err(SendError::Refused(format!("the cloud refused the batch ({status}): {detail}")))
    }
}
