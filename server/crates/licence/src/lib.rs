//! Subscription licensing.
//!
//! Built on the seam that was put in at phase 2: a single guard every mutating
//! route consults. Adding enforcement was one implementation change rather
//! than an edit to every route, which is what the seam was for.
//!
//! The design (docs/ARCHITECTURE.md §5):
//!
//! * A licensing authority holds an **Ed25519 private key**; this binary ships
//!   with only the public half, compiled in rather than read from a file
//!   someone could swap.
//! * A licence is a signed token carrying tenant, plan, seat count, validity
//!   window, hardware fingerprint, and grace period. It verifies with no
//!   internet connection at all.
//! * Clock rollback is caught by a monotonic high-water mark kept in the
//!   Windows Credential Manager and hash-chained locally.
//! * Past the grace period the system degrades to **read-only, never a hard
//!   lock**. An auditor arriving during a lapsed subscription must still be
//!   able to print financial statements. Public financial records are never
//!   held hostage to billing.

#![forbid(unsafe_code)]

pub mod clock;
pub mod fingerprint;
pub mod token;

pub use clock::ClockState;
pub use token::Claims;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// Full function.
    Full,
    /// Within the grace window after expiry: still full function, but the
    /// client shows a countdown.
    Grace { days_remaining: i64 },
    /// Past grace. Everything readable and printable; nothing new recorded.
    ReadOnly,
}

impl Capability {
    pub fn allows_changes(self) -> bool {
        !matches!(self, Capability::ReadOnly)
    }
}

#[derive(Debug, Clone)]
pub struct Licence {
    pub tenant_id: uuid::Uuid,
    pub plan: String,
    pub seats: u32,
    pub expires_at: Option<DateTime<Utc>>,
    pub grace_days: i64,
}

impl Licence {
    /// The licence used when no subscription is in force — a municipality
    /// running its own installation, which is how Gasan starts.
    pub fn unrestricted(tenant_id: uuid::Uuid) -> Self {
        Licence {
            tenant_id,
            plan: "on-premise".into(),
            seats: u32::MAX,
            expires_at: None,
            grace_days: 0,
        }
    }

    pub fn capability_at(&self, now: DateTime<Utc>) -> Capability {
        let Some(expires_at) = self.expires_at else {
            return Capability::Full;
        };

        if now <= expires_at {
            return Capability::Full;
        }

        let days_past = (now - expires_at).num_days();
        if days_past < self.grace_days {
            Capability::Grace { days_remaining: self.grace_days - days_past }
        } else {
            Capability::ReadOnly
        }
    }
}

/// Consulted by every route that changes data.
///
/// Holds the licence and what the clock was last found to be doing. Both
/// matter: an expiry date is only as trustworthy as the clock it is compared
/// against, so a clock that has moved backwards is treated exactly as expiry
/// is — read-only, and recoverable the moment it is corrected.
#[derive(Debug, Clone)]
pub struct LicenceGuard {
    licence: Licence,
    clock: ClockState,
}

impl LicenceGuard {
    pub fn new(licence: Licence) -> Self {
        LicenceGuard { licence, clock: ClockState::Trusted }
    }

    pub fn with_clock(licence: Licence, clock: ClockState) -> Self {
        LicenceGuard { licence, clock }
    }

    pub fn capability(&self) -> Capability {
        if !self.clock.is_trusted() {
            return Capability::ReadOnly;
        }
        self.licence.capability_at(Utc::now())
    }

    pub fn clock(&self) -> ClockState {
        self.clock
    }

    pub fn licence(&self) -> &Licence {
        &self.licence
    }

    /// Why the system is read-only, in words for the office.
    ///
    /// Every one of these says what to do next. A message that only announces
    /// a problem turns into a phone call.
    pub fn explanation(&self) -> Option<String> {
        match (self.clock, self.capability()) {
            (ClockState::Tampered, _) => Some(
                "This computer's record of the date has been altered, so the \
                 subscription cannot be checked. Everything can still be viewed and \
                 printed. Contact the system administrator."
                    .into(),
            ),

            (ClockState::MovedBackwards { by_days }, _) => Some(format!(
                "This computer's clock is set {by_days} days earlier than a date this \
                 system has already seen, so the subscription cannot be checked. \
                 Everything can still be viewed and printed. Correct the date and time \
                 on this computer, and recording will resume."
            )),

            (_, Capability::ReadOnly) => Some(
                "The subscription for this system has ended. Everything can still be \
                 viewed and printed, including all reports and financial statements, \
                 but nothing new can be recorded until it is renewed."
                    .into(),
            ),

            (_, Capability::Grace { days_remaining }) => Some(format!(
                "The subscription for this system has ended. Recording continues for \
                 {days_remaining} more day{}, after which the system becomes view-only \
                 until it is renewed.",
                if days_remaining == 1 { "" } else { "s" }
            )),

            (_, Capability::Full) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    #[test]
    fn an_on_premise_installation_never_expires() {
        let l = Licence::unrestricted(uuid::Uuid::nil());
        assert_eq!(l.capability_at(at(2099, 1, 1)), Capability::Full);
    }

    #[test]
    fn expiry_degrades_through_grace_to_read_only() {
        let l = Licence {
            tenant_id: uuid::Uuid::nil(),
            plan: "standard".into(),
            seats: 10,
            expires_at: Some(at(2026, 8, 1)),
            grace_days: 14,
        };

        assert_eq!(l.capability_at(at(2026, 7, 31)), Capability::Full);
        assert_eq!(l.capability_at(at(2026, 8, 5)), Capability::Grace { days_remaining: 10 });
        assert_eq!(l.capability_at(at(2026, 9, 1)), Capability::ReadOnly);
    }

    #[test]
    fn read_only_still_permits_reading() {
        // The point of the whole design: expiry must never prevent an auditor
        // from printing the books.
        assert!(!Capability::ReadOnly.allows_changes());
    }
}
