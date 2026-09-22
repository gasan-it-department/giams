//! Where the subscription is enforced.
//!
//! One layer, not a check on every route. A per-route check is a check
//! somebody forgets on the route they add next year, and the one they forget
//! is the one that matters.
//!
//! ## What read-only means here
//!
//! Anything that changes data is refused. Everything else — every list, every
//! report, every financial statement, every export, signing in, signing out —
//! continues to work exactly as before.
//!
//! That is the whole design. A COA auditor arriving during a lapsed
//! subscription must still be able to print the books. Public financial
//! records are never held hostage to billing, and a system that did so would
//! deserve to be thrown out of the office.
//!
//! Two routes stay open even in read-only, and both are load-bearing:
//! authentication, because somebody has to be able to sign in to see the
//! notice, and installing a licence, because otherwise renewing would require
//! the very state that renewing is meant to restore.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use giams_licence::{Capability, LicenceGuard};
use tokio::sync::RwLock;

use crate::{AppError, AppState};

/// The licence in force, replaceable while the server runs.
pub type SharedLicence = Arc<RwLock<LicenceGuard>>;

/// Routes that must keep working when the subscription has lapsed.
fn always_permitted(path: &str) -> bool {
    path.starts_with("/api/auth/")
        // Renewing must not require a valid subscription. A renewal path
        // locked behind the thing it renews is a locked door with the key
        // inside it.
        || path == "/api/licence"
}

/// Refuses changes when the subscription is not in force.
///
/// Keyed on the HTTP method rather than on a list of routes: every route that
/// changes something uses POST, PUT, PATCH, or DELETE, and every route that
/// only reads uses GET. A new route is covered the moment it is written.
pub async fn enforce(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let changes_data = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );

    if !changes_data || always_permitted(request.uri().path()) {
        return next.run(request).await;
    }

    let guard = state.licence.read().await;

    if guard.capability().allows_changes() {
        drop(guard);
        return next.run(request).await;
    }

    let message = guard
        .explanation()
        .unwrap_or_else(|| "This system is view-only at the moment.".into());
    drop(guard);

    AppError::Rule {
        code: "SUBSCRIPTION_NOT_IN_FORCE",
        message,
        hint: Some(
            "Everything can still be viewed, printed, and exported. Ask the system \
             administrator to renew the subscription."
                .into(),
        ),
    }
    .into_response()
}

/// What the client is told about the subscription.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LicenceStatus {
    /// `full`, `grace`, or `read_only`.
    pub state: &'static str,
    pub plan: String,
    pub seats: u32,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub days_remaining: Option<i64>,
    /// Present whenever there is something the office should know.
    pub notice: Option<String>,
    /// Whether new records can be made right now.
    pub can_record: bool,
    /// True when the machine's clock cannot be believed, which is a different
    /// problem from an expired subscription and needs a different fix.
    pub clock_problem: bool,
}

pub fn status_of(guard: &LicenceGuard) -> LicenceStatus {
    let capability = guard.capability();
    let licence = guard.licence();

    LicenceStatus {
        state: match capability {
            Capability::Full => "full",
            Capability::Grace { .. } => "grace",
            Capability::ReadOnly => "read_only",
        },
        plan: licence.plan.clone(),
        seats: licence.seats,
        expires_at: licence.expires_at,
        days_remaining: match capability {
            Capability::Grace { days_remaining } => Some(days_remaining),
            Capability::Full => licence
                .expires_at
                .map(|e| (e - chrono::Utc::now()).num_days()),
            Capability::ReadOnly => None,
        },
        notice: guard.explanation(),
        can_record: capability.allows_changes(),
        clock_problem: !guard.clock().is_trusted(),
    }
}
