//! The background uploader.
//!
//! Ships whatever is waiting, on an interval, and reports what it did so the
//! client can show it. Two things it must never do:
//!
//! * **Block a clerk.** A failed upload is not a failed voucher. Everything
//!   here runs on its own task and surfaces as status, never as an error on a
//!   form.
//! * **Push through a hole.** If gap detection halts, the uploader stays
//!   halted until somebody looks. It does not retry its way past a missing
//!   record, because the record is still missing on the next attempt and the
//!   only thing retrying achieves is burying the message.

use std::sync::Arc;

use giams_db::sqlx::PgPool;
use giams_sync::{NodeKey, Outcome};
use tokio::sync::RwLock;
use uuid::Uuid;

/// What the client is shown about uploading.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncStatus {
    /// Whether uploading happens on its own, or only when asked.
    pub automatic: bool,
    /// Records not yet acknowledged by the cloud.
    pub pending: i64,
    pub last_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_success_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Set only when something is wrong, in words the office can act on.
    pub problem: Option<String>,
    /// True once gap detection has stopped uploading. Needs a person.
    pub halted: bool,
    /// Whether a cloud has been configured at all.
    pub configured: bool,
}

/// Shared state between the background task and the status route.
pub struct Uploader {
    pool: PgPool,
    node_id: Uuid,
    tenant_id: Uuid,
    cloud_url: Option<String>,
    key: Option<Arc<NodeKey>>,
    /// Records per batch. Smaller on a slow or metered link, where a failed
    /// large batch wastes the whole transfer and starts again.
    batch_size: i32,
    state: RwLock<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    automatic: bool,
    last_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
    last_success_at: Option<chrono::DateTime<chrono::Utc>>,
    problem: Option<String>,
    halted: bool,
}

impl Uploader {
    pub fn new(
        pool: PgPool,
        node_id: Uuid,
        tenant_id: Uuid,
        cloud_url: Option<String>,
        key: Option<Arc<NodeKey>>,
        automatic: bool,
    ) -> Self {
        let batch_size = std::env::var("GIAMS_UPLOAD_BATCH_SIZE")
            .ok()
            .and_then(|v| v.parse::<i32>().ok())
            .filter(|n| *n >= 1)
            .unwrap_or(500);

        Uploader {
            pool,
            node_id,
            tenant_id,
            cloud_url,
            key,
            batch_size,
            state: RwLock::new(Inner { automatic, ..Inner::default() }),
        }
    }

    pub async fn status(&self) -> SyncStatus {
        let inner = self.state.read().await;
        let pending = giams_sync::pending_count(&self.pool, self.node_id).await.unwrap_or(0);

        SyncStatus {
            automatic: inner.automatic,
            pending,
            last_attempt_at: inner.last_attempt_at,
            last_success_at: inner.last_success_at,
            problem: inner.problem.clone(),
            halted: inner.halted,
            configured: self.cloud_url.is_some() && self.key.is_some(),
        }
    }

    /// Turns automatic uploading on or off. Turning it on also clears a halt,
    /// because the person doing it is saying they have looked.
    pub async fn set_automatic(&self, automatic: bool) {
        let mut inner = self.state.write().await;
        inner.automatic = automatic;
        if automatic {
            inner.halted = false;
            inner.problem = None;
        }
    }

    /// Uploads everything waiting, one batch at a time.
    ///
    /// Bounded rather than "until empty": a node with a year of backlog would
    /// otherwise hold its own database busy for an hour on the first
    /// connection it gets, which is precisely when a clerk is trying to work.
    pub async fn run_now(&self, max_batches: usize) -> SyncStatus {
        let (Some(cloud_url), Some(key)) = (self.cloud_url.as_deref(), self.key.as_ref()) else {
            let mut inner = self.state.write().await;
            inner.problem = Some(
                "No cloud service has been set up for this municipality yet, so nothing is \
                 being uploaded. Everything is recorded here and is safe."
                    .into(),
            );
            drop(inner);
            return self.status().await;
        };

        {
            let mut inner = self.state.write().await;
            inner.last_attempt_at = Some(chrono::Utc::now());
        }

        for _ in 0..max_batches {
            let outcome = giams_sync::ship_once(
                &self.pool,
                self.node_id,
                self.tenant_id,
                cloud_url,
                key,
                self.batch_size,
            )
            .await;

            let mut inner = self.state.write().await;

            match outcome {
                Ok(Outcome::UpToDate) => {
                    inner.problem = None;
                    inner.halted = false;
                    inner.last_success_at = Some(chrono::Utc::now());
                    break;
                }

                Ok(Outcome::Shipped { batch_id, records, through }) => {
                    tracing::info!(%batch_id, records, through, "uploaded a batch to the cloud");
                    inner.problem = None;
                    inner.halted = false;
                    inner.last_success_at = Some(chrono::Utc::now());
                    // Round again: there may be more waiting.
                }

                Ok(Outcome::Unreachable { reason }) => {
                    tracing::debug!(%reason, "the cloud could not be reached");
                    inner.problem = Some(
                        "The cloud service cannot be reached at the moment. Everything is \
                         recorded here and will be uploaded automatically when the connection \
                         returns."
                            .into(),
                    );
                    break;
                }

                Ok(Outcome::Rejected { batch_id, reason }) => {
                    tracing::error!(%batch_id, %reason, "the cloud refused a batch");
                    inner.problem = Some(
                        "The cloud service refused the last upload. Nothing has been lost, but \
                         this needs to be reported to the system administrator."
                            .into(),
                    );
                    break;
                }

                Ok(Outcome::Halted { reason }) => {
                    tracing::error!(%reason, "uploading has been halted");
                    inner.halted = true;
                    inner.problem = Some(
                        "Uploading has been stopped because records are missing from the \
                         upload queue. Nothing already in the books has been affected. Report \
                         this to the system administrator before uploading again."
                            .into(),
                    );
                    break;
                }

                Err(e) => {
                    tracing::error!(error = ?e, "the upload could not be attempted");
                    inner.problem = Some(
                        "The last upload could not be attempted. Everything is recorded here \
                         and is safe."
                            .into(),
                    );
                    break;
                }
            }
        }

        self.status().await
    }
}

/// Starts the loop that uploads on its own.
///
/// Every five minutes rather than continuously: this runs on a provincial
/// connection that is often a phone tether, and a node that reconnects every
/// few seconds costs the office real money in data.
pub fn spawn(uploader: Arc<Uploader>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(300));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            let inner_automatic = uploader.state.read().await.automatic;
            let halted = uploader.state.read().await.halted;

            if inner_automatic && !halted {
                uploader.run_now(20).await;
            }
        }
    });
}
