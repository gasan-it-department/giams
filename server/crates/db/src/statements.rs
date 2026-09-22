//! The general purpose financial statements.
//!
//! Thin wrappers over the functions in migration 0017. Nothing is recomputed
//! here: a statement that disagreed with the ledger it summarises would be
//! wrong in the worst way, silently and for however long nobody checked.
//!
//! Three of the six GPFS are produced. The Statement of Cash Flows and the
//! Statement of Changes in Net Assets/Equity need prior-period opening
//! balances to mean anything, which arrive with the legacy migration.

use anyhow::Context;
use giams_domain::Money;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{scoped, Scope};

#[derive(Debug, sqlx::FromRow)]
struct PositionRow {
    section: String,
    account_code: String,
    account_name: String,
    amount: Decimal,
    is_total: bool,
}

#[derive(Debug, Serialize)]
pub struct StatementLine {
    pub section: String,
    pub account_code: String,
    pub account_name: String,
    pub amount: Money,
    /// Totals and the balancing check are ruled off from the detail.
    pub is_total: bool,
}

#[derive(Debug, Serialize)]
pub struct FinancialPosition {
    pub municipality: String,
    pub fund_name: String,
    pub as_of: chrono::NaiveDate,
    pub lines: Vec<StatementLine>,
    /// Assets less liabilities and equity. Anything but zero and the statement
    /// does not balance, which is the first thing to know about it.
    pub check: Money,
}

pub async fn financial_position(
    pool: &PgPool,
    scope: Scope,
    as_of: chrono::NaiveDate,
    fund_id: Option<Uuid>,
) -> anyhow::Result<FinancialPosition> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, PositionRow>(
        "SELECT section, account_code, account_name, amount, is_total
           FROM statement_financial_position($1, $2, $3)",
    )
    .bind(scope.tenant_id)
    .bind(as_of)
    .bind(fund_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not produce the statement of financial position")?;

    let (municipality, fund_name) = heading(&mut tx, scope, fund_id).await?;
    tx.commit().await?;

    let check = rows
        .iter()
        .find(|r| r.section == "Check")
        .map(|r| r.amount)
        .unwrap_or(Decimal::ZERO);

    Ok(FinancialPosition {
        municipality,
        fund_name,
        as_of,
        // The check row drives the banner rather than appearing as a line.
        lines: rows
            .into_iter()
            .filter(|r| r.section != "Check")
            .map(|r| StatementLine {
                section: r.section,
                account_code: r.account_code,
                account_name: r.account_name,
                amount: Money::new(r.amount),
                is_total: r.is_total,
            })
            .collect(),
        check: Money::new(check),
    })
}

#[derive(Debug, sqlx::FromRow)]
struct PerformanceRow {
    section: String,
    account_code: String,
    account_name: String,
    expense_class: Option<String>,
    amount: Decimal,
    is_total: bool,
}

#[derive(Debug, Serialize)]
pub struct PerformanceLine {
    pub section: String,
    pub account_code: String,
    pub account_name: String,
    pub expense_class: Option<String>,
    pub amount: Money,
    pub is_total: bool,
}

#[derive(Debug, Serialize)]
pub struct FinancialPerformance {
    pub municipality: String,
    pub fund_name: String,
    pub period_label: String,
    pub lines: Vec<PerformanceLine>,
}

pub async fn financial_performance(
    pool: &PgPool,
    scope: Scope,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    fund_id: Option<Uuid>,
) -> anyhow::Result<FinancialPerformance> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, PerformanceRow>(
        "SELECT section, account_code, account_name, expense_class, amount, is_total
           FROM statement_financial_performance($1, $2, $3, $4)",
    )
    .bind(scope.tenant_id)
    .bind(from)
    .bind(to)
    .bind(fund_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not produce the statement of financial performance")?;

    let (municipality, fund_name) = heading(&mut tx, scope, fund_id).await?;
    tx.commit().await?;

    Ok(FinancialPerformance {
        municipality,
        fund_name,
        period_label: format!("{from} to {to}"),
        lines: rows
            .into_iter()
            .map(|r| PerformanceLine {
                section: r.section,
                account_code: r.account_code,
                account_name: r.account_name,
                expense_class: r.expense_class,
                amount: Money::new(r.amount),
                is_total: r.is_total,
            })
            .collect(),
    })
}

#[derive(Debug, sqlx::FromRow)]
struct BudgetActualRow {
    expense_class: String,
    office_name: String,
    account_code: String,
    account_name: String,
    original_budget: Decimal,
    final_budget: Decimal,
    obligated: Decimal,
    disbursed: Decimal,
    unused_budget: Decimal,
    utilisation_rate: Decimal,
}

#[derive(Debug, Serialize)]
pub struct BudgetActualLine {
    pub expense_class: String,
    pub office_name: String,
    pub account_code: String,
    pub account_name: String,
    pub original_budget: Money,
    pub final_budget: Money,
    pub obligated: Money,
    pub disbursed: Money,
    pub unused_budget: Money,
    /// A percentage, not money — sent as a plain string.
    pub utilisation_rate: String,
}

#[derive(Debug, Serialize)]
pub struct BudgetActual {
    pub municipality: String,
    pub fund_name: String,
    pub fiscal_year: i16,
    pub lines: Vec<BudgetActualLine>,
}

/// The Statement of Comparison of Budget and Actual Amounts.
///
/// Unique to the public sector, and the one COA looks at first for an LGU
/// because it ties the appropriation ordinance to what was actually spent.
pub async fn budget_vs_actual(
    pool: &PgPool,
    scope: Scope,
    fiscal_year: i16,
    fund_id: Option<Uuid>,
) -> anyhow::Result<BudgetActual> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, BudgetActualRow>(
        "SELECT expense_class, office_name, account_code, account_name,
                original_budget, final_budget, obligated, disbursed,
                unused_budget, utilisation_rate
           FROM statement_budget_vs_actual($1, $2, $3)",
    )
    .bind(scope.tenant_id)
    .bind(fiscal_year)
    .bind(fund_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not produce the comparison of budget and actual amounts")?;

    let (municipality, fund_name) = heading(&mut tx, scope, fund_id).await?;
    tx.commit().await?;

    Ok(BudgetActual {
        municipality,
        fund_name,
        fiscal_year,
        lines: rows
            .into_iter()
            .map(|r| BudgetActualLine {
                expense_class: r.expense_class,
                office_name: r.office_name,
                account_code: r.account_code,
                account_name: r.account_name,
                original_budget: Money::new(r.original_budget),
                final_budget: Money::new(r.final_budget),
                obligated: Money::new(r.obligated),
                disbursed: Money::new(r.disbursed),
                unused_budget: Money::new(r.unused_budget),
                utilisation_rate: r.utilisation_rate.to_string(),
            })
            .collect(),
    })
}

async fn heading(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    scope: Scope,
    fund_id: Option<Uuid>,
) -> anyhow::Result<(String, String)> {
    let municipality: String = sqlx::query_scalar("SELECT name FROM tenant WHERE id = $1")
        .bind(scope.tenant_id)
        .fetch_one(&mut **tx)
        .await
        .context("could not read the municipality name")?;

    let fund_name: String = match fund_id {
        Some(id) => sqlx::query_scalar("SELECT name FROM fund WHERE id = $1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await
            .unwrap_or_else(|_| "All funds".to_string()),
        None => "All funds".to_string(),
    };

    Ok((municipality, fund_name))
}
