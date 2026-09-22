//! Disbursement vouchers.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use giams_db::disbursement::{
    self, DeductionOption, NewVoucher, Voucher, VoucherDeduction, VoucherFilter, VoucherStep,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiResult, AppError, AppState, CurrentUser};
use crate::extract::Json as JsonBody;
use crate::error::database_error;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/vouchers", get(list).post(create))
        .route("/api/vouchers/{id}/deductions", get(deductions))
        .route("/api/vouchers/{id}/advance", post(advance))
        .route("/api/vouchers/{id}/post", post(post_to_ledger))
        .route("/api/deduction-types", get(deduction_types))
}

async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filter): Query<VoucherFilter>,
) -> ApiResult<Json<Vec<Voucher>>> {
    user.require("dv.draft")?;

    let rows = disbursement::list_vouchers(&state.pool, user.scope(), filter)
        .await
        .map_err(database_error)?;

    Ok(Json(rows))
}

async fn deductions(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<VoucherDeduction>>> {
    user.require("dv.draft")?;

    let rows = disbursement::voucher_deductions(&state.pool, user.scope(), id)
        .await
        .map_err(database_error)?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct DeductionQuery {
    applies_to: Option<String>,
}

async fn deduction_types(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<DeductionQuery>,
) -> ApiResult<Json<Vec<DeductionOption>>> {
    user.require("dv.draft")?;

    let rows = disbursement::list_deduction_options(&state.pool, user.scope(), q.applies_to)
        .await
        .map_err(database_error)?;

    Ok(Json(rows))
}

async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(input): JsonBody<NewVoucher>,
) -> ApiResult<Json<Voucher>> {
    user.require("dv.draft")?;

    if input.particulars.trim().is_empty() {
        return Err(AppError::validation(
            "particulars",
            "Describe what is being paid for. This appears on the printed voucher.",
        ));
    }

    if input.gross_amount.is_zero() || input.gross_amount.is_negative() {
        return Err(AppError::validation(
            "gross_amount",
            "Enter an amount greater than zero.",
        ));
    }

    let id = disbursement::create_voucher(&state.pool, user.scope(), &user.username, input)
        .await
        .map_err(database_error)?;

    fetch_one(&state, &user, id).await
}

#[derive(Debug, Deserialize)]
pub struct AdvanceRequest {
    step: VoucherStep,
}

/// Moves a voucher one step along: submit, certify, approve.
///
/// Each step needs a different permission, because each is a different
/// officer. Approving is separated from certifying deliberately — they are the
/// Accountant and the approving authority, and one person doing both defeats
/// the control the two signatures exist for.
async fn advance(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    JsonBody(body): JsonBody<AdvanceRequest>,
) -> ApiResult<Json<Voucher>> {
    match body.step {
        VoucherStep::Submit => user.require("dv.draft")?,
        VoucherStep::Certify | VoucherStep::Approve => user.require("dv.approve")?,
    }

    let moved = disbursement::advance_voucher(&state.pool, user.scope(), &user.username, id, body.step)
        .await
        .map_err(database_error)?;

    if moved.is_none() {
        // The screen is showing a state the voucher has already left.
        return Err(AppError::Rule {
            code: "VOUCHER_STEP_OUT_OF_ORDER",
            message: match body.step {
                VoucherStep::Submit => "This voucher is no longer a draft.".into(),
                VoucherStep::Certify => "This voucher has to be submitted before it can be certified.".into(),
                VoucherStep::Approve => "This voucher has to be certified before it can be approved.".into(),
            },
            hint: Some("Reload the page to see where it has got to.".into()),
        });
    }

    fetch_one(&state, &user, id).await
}

async fn post_to_ledger(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Voucher>> {
    user.require("jev.post")?;

    disbursement::post_voucher(&state.pool, user.scope(), &user.username, id)
        .await
        .map_err(database_error)?;

    fetch_one(&state, &user, id).await
}

/// Re-reads a voucher so every response carries the joined view rather than a
/// partial echo of what was sent.
async fn fetch_one(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<Json<Voucher>> {
    let mut rows = disbursement::list_vouchers(&state.pool, user.scope(), VoucherFilter::default())
        .await
        .map_err(database_error)?;

    rows.retain(|v| v.id == id);

    rows.pop().map(Json).ok_or(AppError::NotFound {
        entity: "voucher",
        label: "That voucher".into(),
    })
}
