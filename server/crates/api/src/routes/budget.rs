//! Budget execution: appropriations, allotments, and obligation requests.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use giams_db::budget::{
    self, Appropriation, CertificationBox, NewObligation, Obligation, ObligationFilter, RaodEntry,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiResult, AppError, AppState, CurrentUser};
use crate::extract::Json as JsonBody;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/appropriations", get(list_appropriations))
        .route("/api/allotments/available", get(list_allotment_options))
        .route("/api/obligations", get(list_obligations).post(create_obligation))
        .route("/api/obligations/{id}/certify", post(certify))
        .route("/api/registries/raod", get(raod))
}

/// Defaults to the current year so every screen does not have to ask.
#[derive(Debug, Deserialize)]
pub struct YearQuery {
    fiscal_year: Option<i16>,
    fund_id: Option<Uuid>,
    expense_class: Option<String>,
}

fn resolve_year(requested: Option<i16>) -> i16 {
    use chrono::Datelike;
    requested.unwrap_or_else(|| chrono::Utc::now().year() as i16)
}

async fn list_appropriations(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<Vec<Appropriation>>> {
    user.require("budget.view")?;

    let rows = budget::list_appropriations(
        &state.pool,
        user.scope(),
        resolve_year(q.fiscal_year),
        q.fund_id,
    )
    .await
    .map_err(unwrap_database_error)?;

    Ok(Json(rows))
}

async fn list_allotment_options(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<Vec<budget::AllotmentOption>>> {
    user.require("budget.view")?;

    let rows =
        budget::list_allotment_options(&state.pool, user.scope(), resolve_year(q.fiscal_year))
            .await
            .map_err(unwrap_database_error)?;

    Ok(Json(rows))
}

async fn list_obligations(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filter): Query<ObligationFilter>,
) -> ApiResult<Json<Vec<Obligation>>> {
    user.require("budget.view")?;

    let rows = budget::list_obligations(&state.pool, user.scope(), filter)
        .await
        .map_err(unwrap_database_error)?;

    Ok(Json(rows))
}

async fn create_obligation(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(input): JsonBody<NewObligation>,
) -> ApiResult<Json<Obligation>> {
    user.require("obligation.draft")?;

    if input.particulars.trim().is_empty() {
        return Err(AppError::validation(
            "particulars",
            "Describe what this obligation is for. This appears on the printed form.",
        ));
    }

    if input.amount.is_zero() || input.amount.is_negative() {
        return Err(AppError::validation(
            "amount",
            "Enter an amount greater than zero.",
        ));
    }

    let obligation = budget::create_obligation(&state.pool, user.scope(), &user.username, input)
        .await
        .map_err(unwrap_database_error)?;

    Ok(Json(obligation))
}

#[derive(Debug, Deserialize)]
pub struct CertifyRequest {
    #[serde(rename = "box")]
    which: CertificationBox,
}

/// Signs Box B or Box C of an obligation request.
///
/// The two boxes need different permissions because they are different
/// officers: Box B is the Budget Officer certifying funds are available, Box C
/// is the Accountant certifying the obligation is valid. The database
/// additionally refuses one person signing both.
async fn certify(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    JsonBody(body): JsonBody<CertifyRequest>,
) -> ApiResult<Json<Obligation>> {
    match body.which {
        CertificationBox::Budget => user.require("obligation.certify_budget")?,
        CertificationBox::Accounting => user.require("obligation.certify_accounting")?,
    }

    let obligation =
        budget::certify_obligation(&state.pool, user.scope(), &user.username, id, body.which)
            .await
            .map_err(unwrap_database_error)?;

    // The precondition, stated in the order the form is signed.
    obligation.map(Json).ok_or(AppError::Rule {
        code: "CERTIFICATION_OUT_OF_ORDER",
        message: match body.which {
            CertificationBox::Budget =>
                "That obligation request could not be found.".into(),
            CertificationBox::Accounting =>
                "The Budget Officer has to certify that the allotment is available before the Accountant can certify the obligation.".into(),
        },
        hint: match body.which {
            CertificationBox::Budget => None,
            CertificationBox::Accounting =>
                Some("Ask the Budget Office to sign Box B first.".into()),
        },
    })
}

async fn raod(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<Vec<RaodEntry>>> {
    user.require("report.view")?;

    let rows = budget::registry_raod(
        &state.pool,
        user.scope(),
        resolve_year(q.fiscal_year),
        q.fund_id,
        q.expense_class,
    )
    .await
    .map_err(unwrap_database_error)?;

    Ok(Json(rows))
}

/// Recovers the original database error from an `anyhow` wrapper.
///
/// Without this the ceiling and separation-of-duties messages raised by the
/// triggers — which are already written for a clerk — would be flattened into
/// the generic "something went wrong".
fn unwrap_database_error(err: anyhow::Error) -> AppError {
    match err.downcast::<giams_db::sqlx::Error>() {
        Ok(db) => AppError::Database(db),
        Err(other) => AppError::Unexpected(other),
    }
}
