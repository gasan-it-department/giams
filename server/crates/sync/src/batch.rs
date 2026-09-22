//! What travels between a node and the cloud.
//!
//! One shape, defined once, used by both halves. A sender and a receiver that
//! each describe the wire format in their own words drift apart, and the drift
//! shows up as a rejected batch at three in the morning.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One captured row change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub node_seq: i64,
    /// The table it came from. The receiver checks this against its whitelist
    /// before building any SQL from it.
    pub entity: String,
    pub entity_id: Uuid,
    /// `insert`, `update`, or `delete`.
    pub op: String,
    pub payload: serde_json::Value,
}

/// A signed run of consecutive records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    /// Also the idempotency key. A retry after a lost response carries the
    /// same id, and the receiver replays its original answer.
    pub batch_id: Uuid,
    pub tenant_id: Uuid,
    pub node_id: Uuid,
    pub from_seq: i64,
    pub to_seq: i64,
    pub records: Vec<Record>,
}

impl Batch {
    /// The bytes that are signed and sent — the same bytes, once.
    ///
    /// Both the signature and the transmission use this single serialisation,
    /// so there is no second rendering for the two sides to disagree about.
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(Into::into)
    }
}

/// What the receiver says back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub batch_id: Uuid,
    /// How many records were applied. Zero on a replayed batch that had
    /// already been applied in full.
    pub applied: i64,
    /// The sequence number the cloud now holds for this node.
    pub through: i64,
}

/// The header the signature travels in, base64 standard-encoded.
pub const SIGNATURE_HEADER: &str = "x-giams-signature";
