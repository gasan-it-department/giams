//! Seeing and installing the subscription.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use giams_licence::{token, Licence, LicenceGuard};
use serde::Deserialize;

use crate::licensing::{status_of, LicenceStatus};
use crate::{ApiResult, AppError, AppState, CurrentUser};
use crate::extract::Json as JsonBody;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/licence", get(status).post(install))
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
) -> ApiResult<Json<LicenceStatus>> {
    user.require("tenant.manage")?;

    let public_key = token::authority_public_key()
        .map_err(|e| AppError::Unexpected(e.context("this installation cannot check licences")))?;

    let claims = token::verify(input.token.trim(), &public_key)
        .map_err(|e| AppError::Rule {
            code: "LICENCE_NOT_GENUINE",
            message: e.to_string(),
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

    giams_db::sqlx::query(
        r#"
        INSERT INTO licence (tenant_id, token, plan, seats, not_before, expires_at,
                             grace_days, installed_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (tenant_id) DO UPDATE
                SET token = excluded.token, plan = excluded.plan, seats = excluded.seats,
                    not_before = excluded.not_before, expires_at = excluded.expires_at,
                    grace_days = excluded.grace_days, installed_at = now(),
                    installed_by = excluded.installed_by
        "#,
    )
    .bind(claims.tenant_id)
    .bind(input.token.trim())
    .bind(&claims.plan)
    .bind(i32::try_from(claims.seats).unwrap_or(i32::MAX))
    .bind(claims.not_before)
    .bind(claims.expires_at)
    .bind(i32::try_from(claims.grace_days).unwrap_or(0))
    .bind(user.user_id)
    .execute(&state.pool)
    .await
    .map_err(AppError::Database)?;

    // Takes effect immediately. An office that has just renewed should not
    // have to restart the server to be able to record a voucher.
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

    tracing::info!(
        plan = %claims.plan,
        expires = %claims.expires_at,
        "a licence was installed"
    );

    Ok(Json(status_of(&guard)))
}
