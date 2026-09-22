//! Funds and the chart of accounts.

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use giams_db::reference::{self, Account, AccountFilter, Fund, NewAccount, Page};
use uuid::Uuid;

use crate::{ApiResult, AppError, AppState, CurrentUser};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/funds", get(list_funds))
        .route("/api/accounts", get(list_accounts).post(create_account))
        .route("/api/accounts/{id}", delete(deactivate_account))
}

async fn list_funds(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Fund>>> {
    user.require("account.view")?;

    let funds = reference::list_funds(&state.pool, user.scope())
        .await
        .map_err(AppError::Unexpected)?;

    Ok(Json(funds))
}

async fn list_accounts(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filter): Query<AccountFilter>,
) -> ApiResult<Json<Page<Account>>> {
    user.require("account.view")?;

    let accounts = reference::list_accounts(&state.pool, user.scope(), filter)
        .await
        .map_err(AppError::Unexpected)?;

    Ok(Json(accounts))
}

async fn create_account(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(input): JsonBody<NewAccount>,
) -> ApiResult<Json<Account>> {
    user.require("account.manage")?;
    validate_account(&input)?;

    let account = reference::create_account(&state.pool, user.scope(), &user.username, input)
        .await
        .map_err(unwrap_database_error)?;

    Ok(Json(account))
}

async fn deactivate_account(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    user.require("account.manage")?;

    let changed = reference::deactivate_account(&state.pool, user.scope(), &user.username, id)
        .await
        .map_err(unwrap_database_error)?;

    if !changed {
        return Err(AppError::NotFound {
            entity: "account",
            label: "That account".into(),
        });
    }

    Ok(Json(serde_json::json!({
        "deactivated": true,
        "message": "The account is no longer available for new entries. Past entries are unchanged."
    })))
}

/// Checks the shape of the input before it reaches the database.
///
/// The database enforces these too. This layer exists to produce a message
/// naming the offending field, so the form can highlight it rather than
/// showing a banner.
fn validate_account(input: &NewAccount) -> Result<(), AppError> {
    let code = input.code.trim();

    if code.is_empty() {
        return Err(AppError::validation("code", "Enter the account code."));
    }

    // The Revised Chart of Accounts is 8 digits, conventionally written with
    // separators: 1-07-03-010. Digits and dashes only.
    if !code.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return Err(AppError::validation(
            "code",
            "An account code uses only numbers and dashes, like 1-07-03-010.",
        ));
    }

    if code.chars().filter(char::is_ascii_digit).count() != 8 {
        return Err(AppError::validation(
            "code",
            "An account code has eight digits, like 1-07-03-010.",
        ));
    }

    if input.name.trim().is_empty() {
        return Err(AppError::validation("name", "Enter the account name."));
    }

    if !matches!(
        input.account_type.as_str(),
        "asset" | "liability" | "equity" | "income" | "expense"
    ) {
        return Err(AppError::validation(
            "account_type",
            "Choose whether this is an asset, liability, equity, income, or expense account.",
        ));
    }

    // The database has the same rule; catching it here names the field.
    if input.expense_class.is_some() && input.account_type != "expense" {
        return Err(AppError::validation(
            "expense_class",
            "Only expense accounts have an expense class.",
        ));
    }

    Ok(())
}

/// Recovers the original database error from an `anyhow` wrapper.
///
/// The repository layer uses `anyhow` for context, which would otherwise land
/// in `AppError::Unexpected` and produce a generic message — losing the
/// constraint translation that exists precisely for these cases.
fn unwrap_database_error(err: anyhow::Error) -> AppError {
    match err.downcast::<sqlx::Error>() {
        Ok(db) => AppError::Database(db),
        Err(other) => AppError::Unexpected(other),
    }
}

use giams_db::sqlx;
use crate::extract::Json as JsonBody;
