//! Detecting a clock that has been set back.
//!
//! Every offline expiry check rests on the machine's own clock, and the
//! machine's own clock belongs to whoever has the BIOS. Setting it back to
//! 2020 would otherwise buy an indefinite subscription.
//!
//! The defence is a **high-water mark**: the latest moment this installation
//! has ever observed, written down and hash-chained. Time is only allowed to
//! move forward. If the clock now reads earlier than the mark by more than a
//! small tolerance, the clock is not to be trusted, and an untrusted clock is
//! treated as expiry rather than as licence.
//!
//! ## Why hash-chained
//!
//! A bare "last seen" value can be edited by anyone who finds the file. Each
//! entry commits to the one before it, so an edited mark no longer matches the
//! chain and is detected as tampering — the same construction as the audit
//! trail, and for the same reason.
//!
//! ## What this deliberately does not do
//!
//! It does not brick anything. A clock that has moved backwards produces
//! read-only mode, which is recoverable the moment the clock is corrected. The
//! failure mode of a badly set clock in a municipal office is a flat CMOS
//! battery far more often than it is fraud, and the response has to be
//! proportionate to that.

use anyhow::Context;
use chrono::{DateTime, Duration, Utc};
use giams_db::sqlx::{self, PgPool};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// How far the clock may drift backwards without suspicion.
///
/// Generous on purpose: NTP corrections, daylight saving misconfiguration, and
/// a virtual machine resuming from a snapshot all move a clock backwards by
/// legitimate minutes.
const TOLERANCE: Duration = Duration::hours(26);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockState {
    /// Time has only moved forward.
    Trusted,
    /// The clock reads earlier than a moment already recorded.
    MovedBackwards { by_days: i64 },
    /// The record of previous observations has been altered.
    Tampered,
}

impl ClockState {
    pub fn is_trusted(self) -> bool {
        matches!(self, ClockState::Trusted)
    }
}

/// Records the current moment and reports whether the clock can be believed.
///
/// Called at startup and periodically. Each call both reads and advances the
/// mark, so an installation that runs for a year has a mark a year newer.
pub async fn observe(pool: &PgPool, tenant_id: Uuid, now: DateTime<Utc>) -> anyhow::Result<ClockState> {
    let previous = sqlx::query_as::<_, (DateTime<Utc>, Vec<u8>, i64)>(
        "SELECT observed_at, chain_hash, seq FROM licence_clock
          WHERE tenant_id = $1 ORDER BY seq DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .context("could not read this installation's clock history")?;

    let Some((high_water, previous_hash, seq)) = previous else {
        // First observation. Nothing to compare against, so nothing to doubt.
        write(pool, tenant_id, now, 1, &[]).await?;
        return Ok(ClockState::Trusted);
    };

    // The chain is checked before the clock, because a chain that does not
    // hold makes every value in it meaningless — including the one the clock
    // is about to be compared against.
    let expected = chain_hash(tenant_id, high_water, seq, &previous_hash);
    let stored = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT entry_hash FROM licence_clock WHERE tenant_id = $1 AND seq = $2",
    )
    .bind(tenant_id)
    .bind(seq)
    .fetch_one(pool)
    .await
    .context("could not read this installation's clock history")?;

    if stored != expected {
        return Ok(ClockState::Tampered);
    }

    if now < high_water - TOLERANCE {
        return Ok(ClockState::MovedBackwards {
            by_days: (high_water - now).num_days(),
        });
    }

    // Only ever advanced. A clock that is merely a few hours behind the mark
    // does not move it backwards.
    if now > high_water {
        write(pool, tenant_id, now, seq + 1, &stored).await?;
    }

    Ok(ClockState::Trusted)
}

async fn write(
    pool: &PgPool,
    tenant_id: Uuid,
    at: DateTime<Utc>,
    seq: i64,
    previous_hash: &[u8],
) -> anyhow::Result<()> {
    let hash = chain_hash(tenant_id, at, seq, previous_hash);

    sqlx::query(
        "INSERT INTO licence_clock (tenant_id, seq, observed_at, chain_hash, entry_hash)
              VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, seq) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(seq)
    .bind(at)
    .bind(previous_hash)
    .bind(hash.as_slice())
    .execute(pool)
    .await
    .context("could not record the current time")?;

    Ok(())
}

/// Commits to this observation and every one before it.
///
/// The timestamp is hashed as explicit UTC text rather than through any
/// formatting that consults a session time zone — the same trap the audit
/// chain hit, where a chain verified in one time zone and failed in another.
fn chain_hash(tenant_id: Uuid, at: DateTime<Utc>, seq: i64, previous: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(previous);
    hasher.update(tenant_id.as_bytes());
    hasher.update(seq.to_be_bytes());
    hasher.update(at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true).as_bytes());
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chain_binds_each_entry_to_the_one_before() {
        let tenant = Uuid::new_v4();
        let now = Utc::now();

        let first = chain_hash(tenant, now, 1, &[]);
        let second = chain_hash(tenant, now + Duration::days(1), 2, &first);

        // Changing anything about the earlier entry changes the later one,
        // which is what makes an edit detectable rather than silent.
        let altered_first = chain_hash(tenant, now - Duration::days(400), 1, &[]);
        let altered_second = chain_hash(tenant, now + Duration::days(1), 2, &altered_first);

        assert_ne!(second, altered_second);
    }

    #[test]
    fn a_days_drift_is_tolerated_and_a_years_is_not() {
        let now = Utc::now();
        assert!(now - Duration::hours(2) >= now - TOLERANCE, "ordinary drift was treated as fraud");
        assert!(now - Duration::days(400) < now - TOLERANCE, "a year of rollback was tolerated");
    }
}
