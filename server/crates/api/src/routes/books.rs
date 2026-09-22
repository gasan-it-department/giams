//! The books of accounts and the trial balance.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use giams_db::books::{self, Book, BookPage, NewJournalEntry, TrialBalancePage};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiResult, AppError, AppState, CurrentUser};
use crate::extract::Json as JsonBody;
use crate::error::database_error;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/books/{book}", get(read_book))
        .route("/api/trial-balance", get(trial_balance))
        .route("/api/journal-entries", post(create_entry))
        .route("/api/journal-entries/{id}/post", post(post_entry))
}

#[derive(Debug, Deserialize)]
pub struct BookQuery {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub fund_id: Option<Uuid>,
}

async fn read_book(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(book): axum::extract::Path<String>,
    Query(q): Query<BookQuery>,
) -> ApiResult<Json<BookPage>> {
    user.require("report.view")?;

    let book = match book.to_uppercase().as_str() {
        "GJ" => Book::Gj,
        "CRJ" => Book::Crj,
        "CKDJ" => Book::CkDj,
        "CDJ" => Book::Cdj,
        _ => {
            return Err(AppError::NotFound {
                entity: "book",
                label: "That book of accounts".into(),
            })
        }
    };

    if q.to < q.from {
        return Err(AppError::validation(
            "to",
            "The end of the period is before its start.",
        ));
    }

    let page = books::read_book(&state.pool, user.scope(), book, q.from, q.to, q.fund_id)
        .await
        .map_err(database_error)?;

    Ok(Json(page))
}

#[derive(Debug, Deserialize)]
pub struct TrialBalanceQuery {
    pub as_of: chrono::NaiveDate,
    pub fund_id: Option<Uuid>,
}

async fn trial_balance(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<TrialBalanceQuery>,
) -> ApiResult<Json<TrialBalancePage>> {
    user.require("report.view")?;

    let page = books::trial_balance(&state.pool, user.scope(), q.as_of, q.fund_id)
        .await
        .map_err(database_error)?;

    Ok(Json(page))
}

/// Creates a journal entry voucher in draft.
async fn create_entry(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(input): JsonBody<NewJournalEntry>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require("jev.draft")?;

    if input.particulars.trim().is_empty() {
        return Err(AppError::validation(
            "particulars",
            "Describe what this entry is for. It appears on the printed voucher and in the books.",
        ));
    }

    if input.lines.len() < 2 {
        return Err(AppError::validation(
            "lines",
            "An entry needs at least one debit and one credit.",
        ));
    }

    // Caught here so the message names the line. The database enforces the
    // same rule, and would name the constraint instead.
    for (index, line) in input.lines.iter().enumerate() {
        let both = !line.debit.is_zero() && !line.credit.is_zero();
        let neither = line.debit.is_zero() && line.credit.is_zero();

        if both || neither {
            return Err(AppError::validation(
                "lines",
                format!(
                    "Line {} has {}. Each line is either a debit or a credit.",
                    index + 1,
                    if both { "both a debit and a credit" } else { "no amount" }
                ),
            ));
        }
    }

    let (id, jev_no) =
        books::create_journal_entry(&state.pool, user.scope(), &user.username, input)
            .await
            .map_err(database_error)?;

    Ok(Json(serde_json::json!({
        "id": id,
        "jev_no": jev_no,
        "message": format!("Saved as {jev_no}."),
    })))
}

/// Posts a draft entry to the general ledger.
///
/// The balanced-entry constraint fires at COMMIT inside this call, so an entry
/// that does not balance is refused here with the difference named — that
/// message comes from the database and is already written for a clerk.
async fn post_entry(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require("jev.post")?;

    let posted = books::post_journal_entry(&state.pool, user.scope(), &user.username, id)
        .await
        .map_err(database_error)?;

    match posted {
        Some(jev_no) => Ok(Json(serde_json::json!({
            "posted": true,
            "jev_no": jev_no,
            "message": format!("{jev_no} has been posted to the general ledger."),
        }))),
        None => Err(AppError::Rule {
            code: "ENTRY_NOT_A_DRAFT",
            message: "That entry is not a draft, so it cannot be posted again.".into(),
            hint: Some("Reload the page to see its current state.".into()),
        }),
    }
}
