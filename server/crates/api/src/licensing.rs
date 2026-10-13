//! Where the subscription is enforced.
//!
//! One layer, not a check on every route. A per-route check is a check
//! somebody forgets on the route they add next year, and the one they forget
//! is the one that matters.
//!
//! ## The stages
//!
//! * Seven days before the subscription ends, every user is warned.
//! * After it ends, the system keeps working through the grace period, with a
//!   countdown.
//! * After the grace period it is **locked**: every request is refused. This
//!   is the office's decision (30 September 2026). Nothing is deleted; the
//!   books are all there the moment the subscription is renewed.
//! * A computer clock that cannot be believed only stops recording. A wrong
//!   date is not a billing matter and nobody is locked out over one.
//!
//! A few routes stay open even when locked, and each is load-bearing:
//! signing in, so somebody can read the notice; the licence routes, so a
//! renewal can get in; the licensing check-in, which is how renewals travel;
//! and the admin console, which is not the accounting system.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use giams_licence::{token, Capability, Claims, Licence, LicenceGuard};
use tokio::sync::RwLock;

use crate::{AppError, AppState};

/// The licence in force, replaceable while the server runs.
pub type SharedLicence = Arc<RwLock<LicenceGuard>>;

/// Days a subscription keeps working after it ends, before it locks.
pub const GRACE_DAYS: i64 = 5;

/// Stores a verified licence and puts it in force at once.
///
/// The one place a licence takes effect, whether it arrived by someone
/// pasting a key or by the server collecting a renewal. The caller has
/// already verified it; this checks nothing, it only records and applies.
pub async fn put_in_force(
    state: &AppState,
    token_text: &str,
    claims: &Claims,
    installed_by: Option<uuid::Uuid>,
) -> anyhow::Result<LicenceStatus> {
    giams_db::sqlx::query(
        r#"
        INSERT INTO licence (tenant_id, token, plan, seats, not_before, expires_at,
                             grace_days, installed_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (tenant_id) DO UPDATE
                SET token = excluded.token, plan = excluded.plan, seats = excluded.seats,
                    not_before = excluded.not_before, expires_at = excluded.expires_at,
                    grace_days = excluded.grace_days, installed_at = now(),
                    installed_by = coalesce(excluded.installed_by, licence.installed_by)
        "#,
    )
    .bind(claims.tenant_id)
    .bind(token_text)
    .bind(&claims.plan)
    .bind(i32::try_from(claims.seats).unwrap_or(i32::MAX))
    .bind(claims.not_before)
    .bind(claims.expires_at)
    .bind(i32::try_from(claims.grace_days).unwrap_or(0))
    .bind(installed_by)
    .execute(&state.pool)
    .await?;

    // Takes effect immediately. An office that has just renewed should not
    // have to restart the server to be able to work.
    let mut guard = state.licence.write().await;
    *guard = LicenceGuard::with_clock(
        Licence {
            tenant_id: claims.tenant_id,
            plan: claims.plan.clone(),
            seats: claims.seats,
            expires_at: Some(claims.expires_at),
            grace_days: claims.grace_days,
        },
        guard.clock(),
    );

    Ok(status_of(&guard))
}

/// What checking for a renewal found.
#[derive(Debug, serde::Serialize)]
pub struct CheckIn {
    /// `renewed`, `current`, `no_licence`, `not_configured`, `unreachable`,
    /// or `refused`.
    pub outcome: &'static str,
    /// Written for the office.
    pub message: String,
    pub status: LicenceStatus,
}

#[derive(serde::Deserialize)]
struct CheckInAnswer {
    /// A renewed key when the subscription has moved on; absent when the key
    /// installed here is already current.
    token: Option<String>,
    paid_through: chrono::DateTime<chrono::Utc>,
}

#[derive(serde::Deserialize)]
struct RefusalBody {
    message: String,
}

/// Asks the licensing server whether this municipality's subscription has
/// been renewed, and puts any renewal in force.
///
/// This is how a renewal reaches the office without anybody issuing or
/// installing a new key: the key installed here is shown to the licensing
/// server, which answers with the date the subscription is now paid through,
/// signed. Nothing in the answer is trusted until it verifies against the key
/// compiled into this build and names this municipality.
///
/// Never fails the caller. An unreachable licensing server is an ordinary
/// Tuesday in a provincial office, not an error: the subscription simply
/// carries on until the date it already had.
pub async fn check_in(state: &AppState) -> CheckIn {
    let current = |state: &AppState| {
        let state = state.clone();
        async move { status_of(&*state.licence.read().await) }
    };

    let installed: Option<String> =
        giams_db::sqlx::query_scalar("SELECT token FROM licence WHERE tenant_id = $1")
            .bind(state.node.tenant_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();

    let Some(installed) = installed else {
        return CheckIn {
            outcome: "no_licence",
            message: "No licence key is installed on this server, so there is nothing to renew.".into(),
            status: current(state).await,
        };
    };

    let Some(server) = state.licence_server.as_deref() else {
        return CheckIn {
            outcome: "not_configured",
            message: "This server is not set up to reach the licensing server, so renewals \
                      cannot be collected. Ask the system administrator."
                .into(),
            status: current(state).await,
        };
    };

    let unreachable = |state: &AppState| {
        let state = state.clone();
        async move {
            CheckIn {
                outcome: "unreachable",
                message: "The licensing server could not be reached. The system carries on \
                          until the date shown; try again once the internet connection is back."
                    .into(),
                status: status_of(&*state.licence.read().await),
            }
        }
    };

    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build() {
        Ok(c) => c,
        Err(_) => return unreachable(state).await,
    };

    let response = match client
        .post(format!("{}/api/licensing/check-in", server.trim_end_matches('/')))
        .json(&serde_json::json!({ "token": installed }))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "could not reach the licensing server");
            return unreachable(state).await;
        }
    };

    if !response.status().is_success() {
        let message = response
            .json::<RefusalBody>()
            .await
            .map(|b| b.message)
            .unwrap_or_else(|_| "The licensing server did not accept the check.".into());
        tracing::warn!(%message, "the licensing server refused a check-in");
        return CheckIn { outcome: "refused", message, status: current(state).await };
    }

    let answer = match response.json::<CheckInAnswer>().await {
        Ok(a) => a,
        Err(_) => return unreachable(state).await,
    };

    let Some(renewed) = answer.token else {
        return CheckIn {
            outcome: "current",
            message: format!(
                "Up to date. The subscription runs until {}.",
                describe_end(answer.paid_through)
            ),
            status: current(state).await,
        };
    };

    // Believed only once it verifies and names this municipality. A licensing
    // server that has been impersonated gets no further than this.
    let verified = token::authority_public_key()
        .and_then(|key| token::verify(&renewed, &key))
        .ok()
        .filter(|c| c.tenant_id == state.node.tenant_id)
        .filter(|c| giams_licence::fingerprint::matches(c.fingerprint.as_deref()));

    let Some(claims) = verified else {
        tracing::error!("the licensing server answered with a key that did not verify for this municipality");
        return CheckIn {
            outcome: "refused",
            message: "The renewal received could not be verified, so it was not applied.".into(),
            status: current(state).await,
        };
    };

    match put_in_force(state, &renewed, &claims, None).await {
        Ok(status) => {
            tracing::info!(runs_until = %claims.expires_at, "a renewal was collected from the licensing server");
            CheckIn {
                outcome: "renewed",
                message: format!(
                    "Renewed. The subscription now runs until {}.",
                    describe_end(claims.expires_at)
                ),
                status,
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "a renewal was received but could not be saved");
            CheckIn {
                outcome: "refused",
                message: "The renewal was received but could not be saved. Try again.".into(),
                status: current(state).await,
            }
        }
    }
}

/// An end date as the office reads it; a lifetime key has none worth showing.
fn describe_end(at: chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Datelike;
    if at.year() >= 9999 {
        "no end date — it is a lifetime licence".into()
    } else {
        at.format("%-d %B %Y").to_string()
    }
}

/// Routes that must keep working when the subscription has lapsed.
fn always_permitted(path: &str) -> bool {
    path.starts_with("/api/auth/")
        // Renewing must not require a valid subscription. A renewal path
        // locked behind the thing it renews is a locked door with the key
        // inside it. Covers the status, installing a key, and checking for a
        // renewal.
        || path == "/api/licence"
        || path.starts_with("/api/licence/")
        // How renewals reach a municipality's server, answered by the
        // licensing server whatever that server's own subscription is doing.
        || path.starts_with("/api/licensing/")
        // Account administration and issuing keys record nothing in the
        // ledger, and are not the accounting system.
        || path.starts_with("/api/admin/")
        // The client's connection indicator.
        || path == "/api/health"
}

/// Refuses requests the subscription does not allow.
///
/// When locked, everything is refused. When only the clock is in doubt,
/// changes are refused and reading continues — keyed on the HTTP method, so
/// a new route is covered the moment it is written.
pub async fn enforce(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if always_permitted(request.uri().path()) {
        return next.run(request).await;
    }

    let changes_data = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );

    let guard = state.licence.read().await;
    let capability = guard.capability();

    if capability.allows_use() && (!changes_data || capability.allows_changes()) {
        drop(guard);
        return next.run(request).await;
    }

    let message = guard
        .explanation()
        .unwrap_or_else(|| "This system cannot be used at the moment.".into());
    drop(guard);

    let (code, hint) = if capability.allows_use() {
        (
            "SUBSCRIPTION_NOT_IN_FORCE",
            "Everything can still be viewed and printed. Correct the date and time on this computer.",
        )
    } else {
        (
            "SUBSCRIPTION_ENDED",
            "Ask the system administrator to renew the subscription, then choose Check for renewal.",
        )
    };

    AppError::Rule { code, message, hint: Some(hint.into()) }.into_response()
}

/// What the client is told about the subscription.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LicenceStatus {
    /// `full`, `expiring`, `grace`, `locked`, or `read_only`.
    pub state: &'static str,
    pub plan: String,
    pub seats: u32,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the grace period ends and the system locks.
    pub locks_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Days until expiry while `expiring`; days until the lock during `grace`.
    pub days_remaining: Option<i64>,
    /// Present whenever there is something the office should know.
    pub notice: Option<String>,
    /// Whether new records can be made right now.
    pub can_record: bool,
    /// Whether the system can be used at all. False only when locked.
    pub can_use: bool,
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
            Capability::Expiring { .. } => "expiring",
            Capability::Grace { .. } => "grace",
            Capability::Locked => "locked",
            Capability::ReadOnly => "read_only",
        },
        plan: licence.plan.clone(),
        seats: licence.seats,
        expires_at: licence.expires_at,
        locks_at: licence.locks_at(),
        days_remaining: match capability {
            Capability::Expiring { days_left } => Some(days_left),
            Capability::Grace { days_remaining } => Some(days_remaining),
            Capability::Full => licence.expires_at.map(|e| (e - chrono::Utc::now()).num_days()),
            Capability::Locked | Capability::ReadOnly => None,
        },
        notice: guard.explanation(),
        can_record: capability.allows_changes(),
        can_use: capability.allows_use(),
        clock_problem: !guard.clock().is_trusted(),
    }
}
