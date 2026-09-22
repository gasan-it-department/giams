//! Spreadsheet exports of the books, the trial balance, and the statements.
//!
//! Every export re-reads through the same repository functions the screens
//! use. Nothing is exported from a cached page or from anything the client
//! sent up: a spreadsheet that disagreed with the screen it was taken from
//! would be discovered by an auditor, not by us.
//!
//! Permissions match the screens exactly. An export is a copy of what the
//! person can already see, so `report.view` covers all of it.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use giams_db::books::{self, Book};
use giams_db::statements;
use giams_reports::xlsx::{Cell, Col, SheetBuilder};

use crate::{ApiResult, AppError, AppState, CurrentUser};

use super::books::BookQuery;
use super::statements::{AsOfQuery, PeriodQuery, YearQuery};
use crate::error::database_error;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/books/{book}/export.xlsx", get(export_book))
        .route("/api/trial-balance/export.xlsx", get(export_trial_balance))
        .route("/api/statements/financial-position/export.xlsx", get(export_position))
        .route("/api/statements/financial-performance/export.xlsx", get(export_performance))
        .route("/api/statements/budget-vs-actual/export.xlsx", get(export_budget_actual))
}

async fn export_book(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(book): Path<Book>,
    Query(q): Query<BookQuery>,
) -> ApiResult<impl IntoResponse> {
    user.require("report.view")?;

    let page = books::read_book(&state.pool, user.scope(), book, q.from, q.to, q.fund_id)
        .await
        .map_err(database_error)?;

    let mut sheet = SheetBuilder::new();
    sheet
        .sheet(
            book.title(),
            &format!("{} — {}", page.annex, page.title),
            &[
                page.municipality.clone(),
                format!("Fund: {}", page.fund_name),
                page.period_label.clone(),
            ],
            &[
                Col::Code(12.0),
                Col::Code(20.0),
                Col::Text(46.0),
                Col::Text(26.0),
                Col::Code(14.0),
                Col::Text(34.0),
                Col::Amount,
                Col::Amount,
            ],
        )
        .map_err(spreadsheet_error)?;

    sheet
        .header_row(&[
            "Date", "JEV No.", "Particulars", "Reference / Payee",
            "Account Code", "Account Title", "Debit", "Credit",
        ])
        .map_err(spreadsheet_error)?;

    if page.lines.is_empty() {
        sheet
            .note("Nothing was posted to this book in the period covered.")
            .map_err(spreadsheet_error)?;
    }

    for line in &page.lines {
        let date = line.entry_date.to_string();
        let reference = [line.reference.as_deref(), line.counterparty.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");

        sheet
            .row(&[
                Cell::Code(&date),
                Cell::Code(&line.jev_no),
                Cell::Text(&line.particulars),
                Cell::Text(&reference),
                Cell::Code(&line.account_code),
                Cell::Text(&line.account_name),
                Cell::Amount(line.debit),
                Cell::Amount(line.credit),
            ])
            .map_err(spreadsheet_error)?;
    }

    sheet
        .total_row(&[
            Cell::Text("Total"),
            Cell::Blank, Cell::Blank, Cell::Blank, Cell::Blank, Cell::Blank,
            Cell::Amount(page.total_debit),
            Cell::Amount(page.total_credit),
        ])
        .map_err(spreadsheet_error)?;

    spreadsheet(sheet, &format!("{}-{}-to-{}", book.journal_code(), q.from, q.to))
}

async fn export_trial_balance(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AsOfQuery>,
) -> ApiResult<impl IntoResponse> {
    user.require("report.view")?;

    let page = books::trial_balance(&state.pool, user.scope(), q.as_of, q.fund_id)
        .await
        .map_err(database_error)?;

    let mut sheet = SheetBuilder::new();
    sheet
        .sheet(
            "Trial Balance",
            "TRIAL BALANCE",
            &[
                page.municipality.clone(),
                page.fund_name.clone(),
                format!("As at {}", page.as_of),
            ],
            &[Col::Text(46.0), Col::Code(14.0), Col::Amount, Col::Amount],
        )
        .map_err(spreadsheet_error)?;

    sheet
        .header_row(&["Account Title", "Account No.", "Debit Balance", "Credit Balance"])
        .map_err(spreadsheet_error)?;

    if page.lines.is_empty() {
        sheet
            .note("Nothing has been posted as at this date.")
            .map_err(spreadsheet_error)?;
    }

    for line in &page.lines {
        sheet
            .row(&[
                Cell::Text(&line.account_name),
                Cell::Code(&line.account_code),
                Cell::Amount(line.debit_balance),
                Cell::Amount(line.credit_balance),
            ])
            .map_err(spreadsheet_error)?;
    }

    sheet
        .total_row(&[
            Cell::Text("Total"),
            Cell::Blank,
            Cell::Amount(page.total_debit),
            Cell::Amount(page.total_credit),
        ])
        .map_err(spreadsheet_error)?;

    // Carried into the sheet rather than left for the reader to spot: a trial
    // balance that does not balance must not be filed, and the file travels
    // further than the screen it came from.
    if !page.difference.is_zero() {
        sheet
            .note(&format!(
                "THIS TRIAL BALANCE DOES NOT BALANCE. The debits and credits differ by {}. \
                 It must not be filed.",
                page.difference
            ))
            .map_err(spreadsheet_error)?;
    }

    spreadsheet(sheet, &format!("trial-balance-as-at-{}", q.as_of))
}

async fn export_position(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AsOfQuery>,
) -> ApiResult<impl IntoResponse> {
    user.require("report.view")?;

    let page = statements::financial_position(&state.pool, user.scope(), q.as_of, q.fund_id)
        .await
        .map_err(database_error)?;

    let mut sheet = SheetBuilder::new();
    sheet
        .sheet(
            "Financial Position",
            "STATEMENT OF FINANCIAL POSITION",
            &[
                page.municipality.clone(),
                page.fund_name.clone(),
                format!("As at {}", page.as_of),
            ],
            &[Col::Text(22.0), Col::Text(46.0), Col::Code(14.0), Col::Amount],
        )
        .map_err(spreadsheet_error)?;

    sheet
        .header_row(&["Section", "Account Title", "Account No.", "Amount"])
        .map_err(spreadsheet_error)?;

    for line in &page.lines {
        let cells = [
            Cell::Text(&line.section),
            Cell::Text(&line.account_name),
            Cell::Code(code_or_blank(&line.account_code)),
            Cell::Amount(line.amount),
        ];

        if line.is_total {
            sheet.total_row(&cells).map_err(spreadsheet_error)?;
        } else {
            sheet.row(&cells).map_err(spreadsheet_error)?;
        }
    }

    if !page.check.is_zero() {
        sheet
            .note(&format!(
                "THIS STATEMENT DOES NOT BALANCE. Assets exceed liabilities and equity by {}. \
                 It must not be issued.",
                page.check
            ))
            .map_err(spreadsheet_error)?;
    }

    spreadsheet(sheet, &format!("financial-position-as-at-{}", q.as_of))
}

async fn export_performance(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<PeriodQuery>,
) -> ApiResult<impl IntoResponse> {
    user.require("report.view")?;

    let page =
        statements::financial_performance(&state.pool, user.scope(), q.from, q.to, q.fund_id)
            .await
            .map_err(database_error)?;

    let mut sheet = SheetBuilder::new();
    sheet
        .sheet(
            "Financial Performance",
            "STATEMENT OF FINANCIAL PERFORMANCE",
            &[
                page.municipality.clone(),
                page.fund_name.clone(),
                format!("For the period {}", page.period_label),
            ],
            &[Col::Text(14.0), Col::Text(46.0), Col::Code(14.0), Col::Code(10.0), Col::Amount],
        )
        .map_err(spreadsheet_error)?;

    sheet
        .header_row(&["Section", "Account Title", "Account No.", "Class", "Amount"])
        .map_err(spreadsheet_error)?;

    for line in &page.lines {
        let cells = [
            Cell::Text(&line.section),
            Cell::Text(&line.account_name),
            Cell::Code(code_or_blank(&line.account_code)),
            Cell::Code(line.expense_class.as_deref().unwrap_or("")),
            Cell::Amount(line.amount),
        ];

        if line.is_total {
            sheet.total_row(&cells).map_err(spreadsheet_error)?;
        } else {
            sheet.row(&cells).map_err(spreadsheet_error)?;
        }
    }

    spreadsheet(sheet, &format!("financial-performance-{}-to-{}", q.from, q.to))
}

async fn export_budget_actual(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<impl IntoResponse> {
    user.require("report.view")?;

    let page = statements::budget_vs_actual(&state.pool, user.scope(), q.fiscal_year, q.fund_id)
        .await
        .map_err(database_error)?;

    let mut sheet = SheetBuilder::new();
    sheet
        .sheet(
            "Budget and Actual",
            "STATEMENT OF COMPARISON OF BUDGET AND ACTUAL AMOUNTS",
            &[
                page.municipality.clone(),
                page.fund_name.clone(),
                format!("For the year ended 31 December {}", page.fiscal_year),
            ],
            &[
                Col::Code(10.0),
                Col::Text(38.0),
                Col::Text(38.0),
                Col::Code(14.0),
                Col::Amount,
                Col::Amount,
                Col::Amount,
                Col::Amount,
                Col::Amount,
                Col::Code(10.0),
            ],
        )
        .map_err(spreadsheet_error)?;

    sheet
        .header_row(&[
            "Class", "Office", "Account Title", "Account No.",
            "Original Budget", "Final Budget", "Obligated", "Disbursed",
            "Unused Budget", "Used %",
        ])
        .map_err(spreadsheet_error)?;

    if page.lines.is_empty() {
        sheet
            .note(&format!(
                "No appropriations have been recorded for {}. This statement compares the \
                 appropriation ordinance against what was obligated and paid.",
                page.fiscal_year
            ))
            .map_err(spreadsheet_error)?;
    }

    for line in &page.lines {
        sheet
            .row(&[
                Cell::Code(&line.expense_class),
                Cell::Text(&line.office_name),
                Cell::Text(&line.account_name),
                Cell::Code(&line.account_code),
                Cell::Amount(line.original_budget),
                Cell::Amount(line.final_budget),
                Cell::Amount(line.obligated),
                Cell::Amount(line.disbursed),
                Cell::Amount(line.unused_budget),
                Cell::Code(&line.utilisation_rate),
            ])
            .map_err(spreadsheet_error)?;
    }

    spreadsheet(sheet, &format!("budget-and-actual-{}", page.fiscal_year))
}

/// `TOTAL` and `RESULT` are markers the statement functions use to label their
/// summary rows, not account numbers. Printing them in an account column would
/// have somebody looking them up in the chart of accounts.
fn code_or_blank(code: &str) -> &str {
    match code {
        "TOTAL" | "RESULT" | "CHECK" => "",
        other => other,
    }
}

/// Sends a finished workbook as a download.
fn spreadsheet(mut sheet: SheetBuilder, stem: &str) -> ApiResult<impl IntoResponse> {
    let bytes = sheet.finish().map_err(spreadsheet_error)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
    );

    // The filename is built here, not by the browser, so a saved export says
    // what it is and what period it covers without being renamed.
    let filename = format!("giams-{stem}.xlsx");
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );

    Ok((StatusCode::OK, headers, bytes))
}

fn spreadsheet_error(err: giams_reports::xlsx::XlsxError) -> AppError {
    AppError::Unexpected(anyhow::anyhow!(err).context("could not build the spreadsheet"))
}
