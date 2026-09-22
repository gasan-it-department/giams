//! Bank statements, matching, and reconciliation.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use giams_db::reconciliation::{
    self, BankAccountSummary, MatchOutcome, NewStatement, ReconciliationSummary, Statement,
    StatementLine, Suggestion,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiResult, AppError, AppState, CurrentUser};
use crate::extract::Json as JsonBody;
use crate::error::database_error;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/bank-accounts", get(bank_accounts))
        .route("/api/statements", get(statements).post(import))
        .route("/api/statements/{id}/lines", get(lines))
        .route("/api/statements/{id}/rematch", post(rematch))
        .route("/api/statement-lines/{id}/suggestions", get(suggestions))
        .route("/api/statement-lines/{id}/match", post(match_line))
        .route("/api/statement-lines/{id}/unmatch", post(unmatch_line))
        .route("/api/reconciliations/{id}", get(summary))
}

async fn bank_accounts(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<BankAccountSummary>>> {
    user.require("reconcile.perform")?;

    let rows = reconciliation::list_bank_accounts(&state.pool, user.scope())
        .await
        .map_err(database_error)?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct StatementQuery {
    bank_account_id: Option<Uuid>,
}

async fn statements(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<StatementQuery>,
) -> ApiResult<Json<Vec<Statement>>> {
    user.require("reconcile.perform")?;

    let rows = reconciliation::list_statements(&state.pool, user.scope(), q.bank_account_id)
        .await
        .map_err(database_error)?;

    Ok(Json(rows))
}

/// Imports a statement and matches it in one step.
///
/// The client parses the spreadsheet's structure; every amount arrives as a
/// string and is converted to an exact decimal on this side. A spreadsheet
/// quirk can therefore never become a floating-point peso figure.
async fn import(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(input): JsonBody<NewStatement>,
) -> ApiResult<Json<MatchOutcome>> {
    user.require("reconcile.perform")?;

    if input.lines.is_empty() {
        return Err(AppError::validation(
            "lines",
            "That file has no transactions in it. Check that the right file was chosen.",
        ));
    }

    if input.period_end < input.period_start {
        return Err(AppError::validation(
            "period_end",
            "The statement's end date is before its start date.",
        ));
    }

    // Caught here rather than by the database so the message names the row.
    for (index, line) in input.lines.iter().enumerate() {
        let both = !line.withdrawal.is_zero() && !line.deposit.is_zero();
        let neither = line.withdrawal.is_zero() && line.deposit.is_zero();

        if both || neither {
            return Err(AppError::validation(
                "lines",
                format!(
                    "Row {} has {}. Each line of a bank statement is either money out or money in.",
                    index + 1,
                    if both { "both a withdrawal and a deposit" } else { "no amount" }
                ),
            ));
        }
    }

    let outcome = reconciliation::import_statement(&state.pool, user.scope(), &user.username, input)
        .await
        .map_err(database_error)?;

    Ok(Json(outcome))
}

#[derive(Debug, Deserialize)]
pub struct LineQuery {
    unmatched_only: Option<bool>,
}

async fn lines(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<LineQuery>,
) -> ApiResult<Json<Vec<StatementLine>>> {
    user.require("reconcile.perform")?;

    let rows = reconciliation::list_statement_lines(
        &state.pool,
        user.scope(),
        id,
        q.unmatched_only.unwrap_or(false),
    )
    .await
    .map_err(database_error)?;

    Ok(Json(rows))
}

/// Runs the matcher again over a statement.
///
/// Useful after vouchers or deposits have been recorded that did not exist
/// when the statement was imported. Only unmatched lines are considered, so
/// re-running never disturbs a match a person made.
async fn rematch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require("reconcile.perform")?;

    let (exact, by_amount, unmatched): (i32, i32, i32) =
        giams_db::sqlx::query_as("SELECT * FROM match_bank_statement($1)")
            .bind(id)
            .fetch_one(&state.pool)
            .await
            .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({
        "matched_exact": exact,
        "matched_by_amount": by_amount,
        "still_unmatched": unmatched,
        "message": describe_match_run(exact, by_amount, unmatched),
    })))
}

/// Says what the matcher did, in a sentence.
fn describe_match_run(exact: i32, by_amount: i32, unmatched: i32) -> String {
    let matched = exact + by_amount;

    if matched == 0 && unmatched == 0 {
        return "There was nothing left to match.".into();
    }

    if matched == 0 {
        return format!(
            "Nothing could be matched automatically. {unmatched} line{} need{} to be matched by hand.",
            if unmatched == 1 { "" } else { "s" },
            if unmatched == 1 { "s" } else { "" },
        );
    }

    format!(
        "Matched {matched} line{}{}. {}",
        if matched == 1 { "" } else { "s" },
        if by_amount > 0 {
            format!(", {by_amount} by amount alone")
        } else {
            String::new()
        },
        if unmatched == 0 {
            "Nothing is left over.".to_string()
        } else {
            format!(
                "{unmatched} still need{} to be matched by hand.",
                if unmatched == 1 { "s" } else { "" }
            )
        }
    )
}

async fn suggestions(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Suggestion>>> {
    user.require("reconcile.perform")?;

    let rows = reconciliation::suggest_matches(&state.pool, user.scope(), id)
        .await
        .map_err(database_error)?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct MatchRequest {
    kind: String,
    record_id: Uuid,
}

async fn match_line(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    JsonBody(body): JsonBody<MatchRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require("reconcile.perform")?;

    if !matches!(body.kind.as_str(), "disbursement" | "collection") {
        return Err(AppError::validation(
            "kind",
            "A statement line is matched either to a payment or to a collection.",
        ));
    }

    let matched = reconciliation::match_line_manually(
        &state.pool,
        user.scope(),
        &user.username,
        id,
        &body.kind,
        body.record_id,
    )
    .await
    .map_err(database_error)?;

    if !matched {
        return Err(AppError::Rule {
            code: "LINE_ALREADY_MATCHED",
            message: "That statement line has already been matched.".into(),
            hint: Some("Reload the page to see what it was matched to.".into()),
        });
    }

    Ok(Json(serde_json::json!({ "matched": true })))
}

async fn unmatch_line(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require("reconcile.perform")?;

    let changed = reconciliation::unmatch_line(&state.pool, user.scope(), &user.username, id)
        .await
        .map_err(database_error)?;

    Ok(Json(serde_json::json!({
        "unmatched": changed,
        "message": if changed {
            "The match has been removed. The line is waiting to be matched again."
        } else {
            "That line was not matched to anything."
        }
    })))
}

async fn summary(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ReconciliationSummary>> {
    user.require("reconcile.perform")?;

    reconciliation::reconciliation_summary(&state.pool, user.scope(), id)
        .await
        .map_err(database_error)?
        .map(Json)
        .ok_or(AppError::NotFound {
            entity: "reconciliation",
            label: "That reconciliation".into(),
        })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_match_summary_reads_as_a_sentence() {
        assert_eq!(
            describe_match_run(3, 2, 1),
            "Matched 5 lines, 2 by amount alone. 1 still needs to be matched by hand."
        );
        assert_eq!(
            describe_match_run(4, 0, 0),
            "Matched 4 lines. Nothing is left over."
        );
        assert_eq!(
            describe_match_run(1, 0, 0),
            "Matched 1 line. Nothing is left over."
        );
        assert_eq!(
            describe_match_run(0, 0, 3),
            "Nothing could be matched automatically. 3 lines need to be matched by hand."
        );
        assert_eq!(describe_match_run(0, 0, 0), "There was nothing left to match.");
    }
}
