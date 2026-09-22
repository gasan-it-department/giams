//! Node → cloud replication.
//!
//! Both halves live here: the [`shipper`] that a node runs and the
//! [`receiver`] that the cloud runs, sharing one definition of what travels
//! between them ([`batch`]) and one signing scheme ([`identity`]).
//!
//! The schema is migrations 0006, 0021, 0022, and 0023. Capture is a trigger
//! rather than a call from application code, so a route added later cannot
//! forget to enqueue.
//!
//! See docs/ARCHITECTURE.md §3 for why replication is one-directional:
//! ledger facts flow node → cloud only, which removes the entire class of
//! bidirectional merge conflicts.

#![forbid(unsafe_code)]

pub mod batch;
pub mod enrolment;
pub mod identity;
pub mod receiver;
pub mod shipper;

pub use batch::{Batch, Receipt, Record};
pub use identity::NodeKey;
pub use shipper::{ship_once, Outcome};

use anyhow::Context;
use giams_db::sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct Gap {
    pub missing_from: i64,
    pub missing_to: i64,
}

/// Finds breaks in a node's outbound stream.
///
/// A gap means a record was lost or removed somewhere between this node and
/// the cloud. Sync **must stop** and be investigated rather than skipping
/// ahead: continuing would leave a voucher permanently absent from the cloud
/// books, and nothing downstream would ever indicate it was missing.
pub async fn find_gaps(pool: &PgPool, node_id: Uuid) -> anyhow::Result<Vec<Gap>> {
    sqlx::query_as::<_, Gap>("SELECT missing_from, missing_to FROM sync_find_gaps($1)")
        .bind(node_id)
        .fetch_all(pool)
        .await
        .context("could not check the upload queue for missing records")
}

/// How many changes are waiting to be uploaded.
///
/// Surfaced in the client's status bar, so a clerk can see at a glance that
/// work has not yet reached the cloud.
pub async fn pending_count(pool: &PgPool, node_id: Uuid) -> anyhow::Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM sync_outbox WHERE node_id = $1 AND acked_at IS NULL",
    )
    .bind(node_id)
    .fetch_one(pool)
    .await
    .context("could not count changes waiting to upload")?;

    Ok(count)
}
