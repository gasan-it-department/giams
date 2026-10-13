//! Seeing and installing the subscription.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use giams_db::accounts::{self, NewAccount};
use giams_db::licensees;
use giams_licence::token;
use giams_licence::token::LicensedPerson;
use serde::{Deserialize, Serialize};

use crate::auth::password::{generate_one_time_password, hash_password};
use crate::error::{as_sentence, database_error};
use crate::licensing::{check_in, put_in_force, status_of, CheckIn, LicenceStatus, GRACE_DAYS};
use crate::{ApiResult, AppError, AppState, CurrentUser};
use crate::extract::Json as JsonBody;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/licence", get(status).post(install))
        .route("/api/licence/check", post(check_now))
        .route("/api/licensing/check-in", post(answer_check_in))
}

/// Readable by anyone signed in.
///
/// Not restricted to administrators: a clerk who cannot record a voucher needs
/// to be able to find out why without waiting for somebody with more
/// permissions to come and tell them.
async fn status(State(state): State<AppState>, _user: CurrentUser) -> ApiResult<Json<LicenceStatus>> {
    let guard = state.licence.read().await;
    Ok(Json(status_of(&guard)))
}

#[derive(Debug, Deserialize)]
pub struct InstallInput {
    pub token: String,
}

/// What installing a key did.
#[derive(Serialize)]
pub struct InstallOutcome {
    #[serde(flatten)]
    status: LicenceStatus,
    /// The accounts the key brought with it, if it carried any.
    provisioned: Vec<ProvisionedAccount>,
}

#[derive(Serialize)]
struct ProvisionedAccount {
    username: String,
    full_name: String,
    role: String,
    /// `created`, or `existing` for someone already on this server — a
    /// renewal carries the same people and must not reset their passwords.
    status: &'static str,
    /// Present only for a new account, and only in this response.
    password: Option<String>,
}

/// Installs a licence key.
///
/// Verified before it is stored, and stored only if it is genuine and for this
/// municipality. A key that fails any of those checks is refused with a
/// message saying which — an office typing a key from an email needs to know
/// whether they mistyped it or were sent the wrong one.
async fn install(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(input): JsonBody<InstallInput>,
) -> ApiResult<Json<InstallOutcome>> {
    user.require("tenant.manage")?;

    let public_key = token::authority_public_key()
        .map_err(|e| AppError::Unexpected(e.context("this installation cannot check licences")))?;

    let claims = token::verify(input.token.trim(), &public_key)
        .map_err(|e| AppError::Rule {
            code: "LICENCE_NOT_GENUINE",
            message: as_sentence(&e.to_string()),
            hint: Some("Check the key against the one you were sent.".into()),
        })?;

    if claims.tenant_id != state.node.tenant_id {
        return Err(AppError::Rule {
            code: "LICENCE_FOR_ANOTHER_MUNICIPALITY",
            message: format!(
                "That licence key was issued to {}, not to this municipality.",
                claims.municipality
            ),
            hint: Some("Ask for the key issued for this office.".into()),
        });
    }

    if !giams_licence::fingerprint::matches(claims.fingerprint.as_deref()) {
        return Err(AppError::Rule {
            code: "LICENCE_FOR_ANOTHER_COMPUTER",
            message: "That licence key was issued for a different computer.".into(),
            hint: Some(
                "A key is tied to the server it was issued for. Ask for one issued for \
                 this server."
                    .into(),
            ),
        });
    }

    let now = chrono::Utc::now();
    if now < claims.not_before {
        return Err(AppError::Rule {
            code: "LICENCE_NOT_YET_VALID",
            message: format!(
                "That licence key does not start until {}.",
                claims.not_before.format("%-d %B %Y")
            ),
            hint: Some("It can be installed on or after that date.".into()),
        });
    }

    let mut status = put_in_force(&state, input.token.trim(), &claims, Some(user.user_id))
        .await
        .map_err(AppError::Unexpected)?;

    tracing::info!(
        plan = %claims.plan,
        expires = %claims.expires_at,
        people = claims.people.len(),
        "a licence was installed"
    );

    // After the licence is in force, so a problem creating an account never
    // leaves the office without its subscription. Installing the same key
    // again finishes the job: anyone already here is skipped.
    let provisioned = provision(&state, &user, &claims.people).await?;

    // A key installed months after it was issued may have been renewed since,
    // and the office should not have to find that out by being locked out.
    let check = check_in(&state).await;
    if check.outcome == "renewed" {
        status = check.status;
    }

    Ok(Json(InstallOutcome { status, provisioned }))
}

/// Collects any renewal from the licensing server now, rather than waiting
/// for the next scheduled check.
///
/// Open to anyone signed in, like the status itself: a clerk facing a locked
/// system is exactly who needs this button.
async fn check_now(State(state): State<AppState>, _user: CurrentUser) -> Json<CheckIn> {
    Json(check_in(&state).await)
}

#[derive(Debug, Deserialize)]
struct CheckInRequest {
    token: String,
}

#[derive(Serialize)]
struct CheckInAnswer {
    /// Present when the subscription has moved on from the key presented: the
    /// same key, carried to the new end date and re-signed.
    token: Option<String>,
    paid_through: chrono::DateTime<chrono::Utc>,
}

/// The licensing server's side of a check-in.
///
/// A municipality's server presents the key it has installed; this answers
/// with the date that municipality is paid through, from the renewals
/// recorded in the admin console. Needs no sign-in — holding a genuine key is
/// the credential, and all it can ever learn is its own municipality's date.
async fn answer_check_in(
    State(state): State<AppState>,
    JsonBody(request): JsonBody<CheckInRequest>,
) -> ApiResult<Json<CheckInAnswer>> {
    let issuer = state.issuer.clone().ok_or(AppError::Rule {
        code: "NOT_A_LICENSING_SERVER",
        message: "This server does not issue licences, so it cannot answer for one.".into(),
        hint: None,
    })?;

    let public_key = token::authority_public_key()
        .map_err(|e| AppError::Unexpected(e.context("this installation cannot check licences")))?;

    let presented = token::verify(request.token.trim(), &public_key).map_err(|e| AppError::Rule {
        code: "LICENCE_NOT_GENUINE",
        message: as_sentence(&e.to_string()),
        hint: None,
    })?;

    // The profiles belong to the licensing office's own municipality on this
    // server; the request carries no sign-in to scope by.
    let scope = giams_db::Scope { tenant_id: state.node.tenant_id, user_id: None };

    let standing = licensees::standing_for_tenant(&state.pool, scope, presented.tenant_id)
        .await
        .map_err(database_error)?
        .ok_or(AppError::Rule {
            code: "LICENCE_UNKNOWN",
            message: "The licensing server has no record of this municipality's licence.".into(),
            hint: Some("Ask the licensing office to check the municipality's profile.".into()),
        })?;

    // Never shortens what the office already holds.
    let paid_through = standing.paid_through.max(presented.expires_at);

    // Nothing to send when the key presented already says it all.
    if paid_through == presented.expires_at && presented.grace_days == GRACE_DAYS {
        return Ok(Json(CheckInAnswer { token: None, paid_through }));
    }

    let carried = giams_licence::Claims {
        plan: standing.access,
        issued_at: chrono::Utc::now(),
        expires_at: paid_through,
        grace_days: GRACE_DAYS,
        ..presented
    };

    let renewed = token::issue(&carried, &issuer).map_err(AppError::Unexpected)?;

    tracing::info!(
        municipality = %carried.municipality,
        paid_through = %paid_through,
        "a municipality collected its renewal"
    );

    Ok(Json(CheckInAnswer { token: Some(renewed), paid_through }))
}

/// Creates the accounts a key carries, each with a one-time password.
async fn provision(
    state: &AppState,
    user: &CurrentUser,
    people: &[LicensedPerson],
) -> ApiResult<Vec<ProvisionedAccount>> {
    if people.is_empty() {
        return Ok(Vec::new());
    }

    let mut passwords = Vec::with_capacity(people.len());
    let mut batch = Vec::with_capacity(people.len());

    for person in people {
        let password = generate_one_time_password();
        let hash = hash_password(&password).map_err(AppError::Unexpected)?;
        batch.push((
            NewAccount {
                username: person.username.clone(),
                full_name: person.full_name.clone(),
                position_title: person.position_title.clone(),
                role: person.role.clone(),
            },
            hash,
        ));
        passwords.push(password);
    }

    let outcome = accounts::provision_accounts(&state.pool, user.scope(), &user.username, &batch)
        .await
        .map_err(database_error)?;

    Ok(outcome
        .into_iter()
        .zip(passwords)
        .map(|(o, password)| ProvisionedAccount {
            password: (o.status == "created").then_some(password),
            username: o.username,
            full_name: o.full_name,
            role: o.role,
            status: o.status,
        })
        .collect())
}
