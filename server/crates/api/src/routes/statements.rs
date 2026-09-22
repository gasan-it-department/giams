//! The general purpose financial statements.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use giams_db::statements::{self, BudgetActual, FinancialPerformance, FinancialPosition};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiResult, AppError, AppState, CurrentUser};
use crate::error::database_error;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/statements/financial-position", get(financial_position))
        .route("/api/statements/financial-performance", get(financial_performance))
        .route("/api/statements/budget-vs-actual", get(budget_vs_actual))
}

#[derive(Debug, Deserialize)]
pub struct AsOfQuery {
    pub as_of: chrono::NaiveDate,
    pub fund_id: Option<Uuid>,
}

async fn financial_position(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AsOfQuery>,
) -> ApiResult<Json<FinancialPosition>> {
    user.require("report.view")?;

    let page = statements::financial_position(&state.pool, user.scope(), q.as_of, q.fund_id)
        .await
        .map_err(database_error)?;

    Ok(Json(page))
}

#[derive(Debug, Deserialize)]
pub struct PeriodQuery {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub fund_id: Option<Uuid>,
}

async fn financial_performance(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<PeriodQuery>,
) -> ApiResult<Json<FinancialPerformance>> {
    user.require("report.view")?;

    if q.to < q.from {
        return Err(AppError::validation(
            "to",
            "The end of the period is before its start.",
        ));
    }

    let page =
        statements::financial_performance(&state.pool, user.scope(), q.from, q.to, q.fund_id)
            .await
            .map_err(database_error)?;

    Ok(Json(page))
}

#[derive(Debug, Deserialize)]
pub struct YearQuery {
    pub fiscal_year: i16,
    pub fund_id: Option<Uuid>,
}

async fn budget_vs_actual(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<BudgetActual>> {
    user.require("report.view")?;

    let page = statements::budget_vs_actual(&state.pool, user.scope(), q.fiscal_year, q.fund_id)
        .await
        .map_err(database_error)?;

    Ok(Json(page))
}
