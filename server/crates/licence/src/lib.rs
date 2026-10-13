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
//! * The office is warned from seven days before expiry, keeps working through
//!   the grace period after it, and is then **locked out** until the
//!   subscription is renewed. That was the office's decision (30 September
//!   2026), replacing the earlier view-only degradation. Nothing is deleted:
//!   every record is there, untouched, the moment the subscription resumes.
//! * A clock that cannot be believed is a different problem from an unpaid
//!   subscription, and is still only view-only — nobody is locked out of the
//!   books because a computer's date was wrong.

#![forbid(unsafe_code)]

pub mod clock;
pub mod fingerprint;
pub mod token;

pub use clock::ClockState;
pub use token::Claims;

use chrono::{DateTime, Utc};

/// Days before expiry that every user is warned.
pub const WARN_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// Full function.
    Full,
    /// Full function, but the subscription ends within [`WARN_DAYS`], so every
    /// screen says so.
    Expiring { days_left: i64 },
    /// Past expiry but within the grace window: still full function, with a
    /// countdown to the lock.
    Grace { days_remaining: i64 },
    /// Past grace: the system cannot be used until the subscription is renewed.
    Locked,
    /// The computer's clock cannot be believed. Everything readable, nothing
    /// recorded — a wrong date is not a reason to lock anyone out.
    ReadOnly,
}

impl Capability {
    /// Whether new records can be made.
    pub fn allows_changes(self) -> bool {
        !matches!(self, Capability::Locked | Capability::ReadOnly)
    }

    /// Whether the system can be used at all.
    pub fn allows_use(self) -> bool {
        !matches!(self, Capability::Locked)
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
            // Rounded up: with a day and a half left, the office is told two
            // days, never "0 days" while the system still works.
            let hours_left = (expires_at - now).num_hours();
            let days_left = (hours_left + 23) / 24;
            return if days_left <= WARN_DAYS {
                Capability::Expiring { days_left: days_left.max(1) }
            } else {
                Capability::Full
            };
        }

        let days_past = (now - expires_at).num_days();
        if days_past < self.grace_days {
            Capability::Grace { days_remaining: self.grace_days - days_past }
        } else {
            Capability::Locked
        }
    }

    /// The day the system locks: the end of the grace period.
    pub fn locks_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at.map(|e| e + chrono::Duration::days(self.grace_days))
    }
}

/// Consulted by every request.
///
/// Holds the licence and what the clock was last found to be doing. Both
/// matter: an expiry date is only as trustworthy as the clock it is compared
/// against, so a clock that has moved backwards makes the system view-only —
/// recoverable the moment it is corrected, and never a lockout.
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

    /// What the office should know about the subscription, in words for it.
    ///
    /// Every one of these says what to do next. A message that only announces
    /// a problem turns into a phone call.
    pub fn explanation(&self) -> Option<String> {
        let ends = self
            .licence
            .expires_at
            .map(|e| e.format("%-d %B %Y").to_string())
            .unwrap_or_default();
        let grace = self.licence.grace_days;
        let plural = |n: i64| if n == 1 { "" } else { "s" };

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

            (_, Capability::Locked) => Some(format!(
                "The subscription for this system ended on {ends} and was not renewed, \
                 so the system is locked. Nothing has been deleted: every record will \
                 be there as soon as the subscription is renewed."
            )),

            (_, Capability::Grace { days_remaining }) => Some(format!(
                "The subscription for this system ended on {ends}. It can still be used \
                 for {days_remaining} more day{}; after that it will be locked until the \
                 subscription is renewed.",
                plural(days_remaining)
            )),

            (_, Capability::Expiring { days_left }) => Some(format!(
                "The subscription for this system ends on {ends}, in {days_left} day{}. \
                 Renew it before then: {grace} day{} after it ends, the system will be locked.",
                plural(days_left),
                plural(grace)
            )),

            // Only the clock can make it read-only, and that is handled above.
            (_, Capability::ReadOnly) | (_, Capability::Full) => None,
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

    fn monthly(expires: DateTime<Utc>) -> Licence {
        Licence {
            tenant_id: uuid::Uuid::nil(),
            plan: "monthly".into(),
            seats: 10,
            expires_at: Some(expires),
            grace_days: 5,
        }
    }

    #[test]
    fn a_subscription_warns_then_counts_down_then_locks() {
        // Ends at noon on 1 August.
        let l = monthly(at(2026, 8, 1));

        // More than seven days out: nothing to say.
        assert_eq!(l.capability_at(at(2026, 7, 20)), Capability::Full);
        // Seven days out: every user is warned.
        assert_eq!(l.capability_at(at(2026, 7, 25)), Capability::Expiring { days_left: 7 });
        assert_eq!(l.capability_at(at(2026, 7, 31)), Capability::Expiring { days_left: 1 });
        // Ended, within the five days' grace: still works, counting down.
        assert_eq!(l.capability_at(at(2026, 8, 2)), Capability::Grace { days_remaining: 4 });
        assert_eq!(l.capability_at(at(2026, 8, 5)), Capability::Grace { days_remaining: 1 });
        // Five days after it ended: locked.
        assert_eq!(l.capability_at(at(2026, 8, 6)), Capability::Locked);
        assert_eq!(l.capability_at(at(2026, 9, 30)), Capability::Locked);
        assert_eq!(l.locks_at(), Some(at(2026, 8, 6)));
    }

    #[test]
    fn locked_means_unusable_but_a_bad_clock_only_stops_recording() {
        assert!(!Capability::Locked.allows_use());
        assert!(!Capability::Locked.allows_changes());

        assert!(Capability::ReadOnly.allows_use());
        assert!(!Capability::ReadOnly.allows_changes());

        for working in [
            Capability::Full,
            Capability::Expiring { days_left: 3 },
            Capability::Grace { days_remaining: 2 },
        ] {
            assert!(working.allows_use() && working.allows_changes(), "{working:?}");
        }
    }

    #[test]
    fn every_stage_after_the_warning_explains_itself() {
        // `explanation` reads the real clock, so each licence is built
        // relative to now rather than to a fixed date.
        let soon = LicenceGuard::new(monthly(Utc::now() + chrono::Duration::days(3)));
        assert!(soon.explanation().unwrap().contains("will be locked"));

        let grace = LicenceGuard::new(monthly(Utc::now() - chrono::Duration::days(2)));
        assert!(grace.explanation().unwrap().contains("more day"));

        let locked = LicenceGuard::new(monthly(Utc::now() - chrono::Duration::days(30)));
        let text = locked.explanation().unwrap();
        assert!(text.contains("locked") && text.contains("Nothing has been deleted"), "{text}");

        let fine = LicenceGuard::new(monthly(Utc::now() + chrono::Duration::days(40)));
        assert!(fine.explanation().is_none());
    }
}
