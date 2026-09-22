//! Bank statements, matching, and reconciliation.
//!
//! The matching itself lives in the database (migration 0016) and is not
//! reimplemented here. This layer imports statements, runs the matcher, and
//! reads back what it decided.
//!
//! Amounts arrive from a spreadsheet as text and are converted **here**, once,
//! into exact decimals. The client parses the file's *structure*; it never
//! parses a peso figure.

use anyhow::Context;
use giams_domain::Money;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{scoped, Scope};

// ── Statements ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BankAccountSummary {
    pub id: Uuid,
    pub bank_name: String,
    pub account_no: String,
    pub account_name: String,
    pub fund_name: String,
}

pub async fn list_bank_accounts(
    pool: &PgPool,
    scope: Scope,
) -> anyhow::Result<Vec<BankAccountSummary>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, BankAccountSummary>(
        r#"
        SELECT b.id, b.bank_name, b.account_no, b.account_name, f.name AS fund_name
          FROM bank_account b
          JOIN fund f ON f.id = b.fund_id
         WHERE b.is_active
         ORDER BY b.bank_name, b.account_no
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .context("could not read the bank accounts")?;

    tx.commit().await?;
    Ok(rows)
}

#[derive(Debug, sqlx::FromRow)]
struct StatementRow {
    id: Uuid,
    bank_name: String,
    account_no: String,
    statement_date: chrono::NaiveDate,
    period_start: chrono::NaiveDate,
    period_end: chrono::NaiveDate,
    opening_balance: Decimal,
    closing_balance: Decimal,
    line_count: i64,
    unmatched_count: i64,
}

#[derive(Debug, Serialize)]
pub struct Statement {
    pub id: Uuid,
    pub bank_name: String,
    pub account_no: String,
    pub statement_date: chrono::NaiveDate,
    pub period_start: chrono::NaiveDate,
    pub period_end: chrono::NaiveDate,
    pub opening_balance: Money,
    pub closing_balance: Money,
    pub line_count: i64,
    /// What still needs a person. The number that decides whether this
    /// statement is done or not.
    pub unmatched_count: i64,
}

pub async fn list_statements(
    pool: &PgPool,
    scope: Scope,
    bank_account_id: Option<Uuid>,
) -> anyhow::Result<Vec<Statement>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, StatementRow>(
        r#"
        SELECT s.id, b.bank_name, b.account_no, s.statement_date,
               s.period_start, s.period_end, s.opening_balance, s.closing_balance,
               (SELECT count(*) FROM bank_statement_line l WHERE l.statement_id = s.id) AS line_count,
               (SELECT count(*) FROM bank_statement_line l
                 WHERE l.statement_id = s.id AND l.matched_id IS NULL) AS unmatched_count
          FROM bank_statement s
          JOIN bank_account b ON b.id = s.bank_account_id
         WHERE ($1::uuid IS NULL OR s.bank_account_id = $1)
         ORDER BY s.period_end DESC
        "#,
    )
    .bind(bank_account_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not read the bank statements")?;

    tx.commit().await?;

    Ok(rows
        .into_iter()
        .map(|r| Statement {
            id: r.id,
            bank_name: r.bank_name,
            account_no: r.account_no,
            statement_date: r.statement_date,
            period_start: r.period_start,
            period_end: r.period_end,
            opening_balance: Money::new(r.opening_balance),
            closing_balance: Money::new(r.closing_balance),
            line_count: r.line_count,
            unmatched_count: r.unmatched_count,
        })
        .collect())
}

/// One row as it came off the spreadsheet.
///
/// Amounts are strings. The client parses the file's shape; converting text to
/// an exact decimal happens on this side, so a spreadsheet quirk cannot become
/// a floating-point amount.
#[derive(Debug, Deserialize)]
pub struct ImportedLine {
    pub txn_date: chrono::NaiveDate,
    pub description: String,
    pub reference: Option<String>,
    pub withdrawal: Money,
    pub deposit: Money,
}

#[derive(Debug, Deserialize)]
pub struct NewStatement {
    pub bank_account_id: Uuid,
    pub statement_date: chrono::NaiveDate,
    pub period_start: chrono::NaiveDate,
    pub period_end: chrono::NaiveDate,
    pub opening_balance: Money,
    pub closing_balance: Money,
    pub source_filename: Option<String>,
    pub lines: Vec<ImportedLine>,
}

#[derive(Debug, Serialize)]
pub struct MatchOutcome {
    pub statement_id: Uuid,
    pub matched_exact: i32,
    pub matched_by_amount: i32,
    pub still_unmatched: i32,
}

/// Imports a statement and immediately runs the matcher over it.
///
/// One transaction: a half-imported statement with some lines matched would be
/// worse than none at all, because it looks like work already done.
pub async fn import_statement(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    input: NewStatement,
) -> anyhow::Result<MatchOutcome> {
    let mut tx = scoped(pool, scope).await?;

    let statement_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO bank_statement
            (tenant_id, bank_account_id, statement_date, period_start, period_end,
             opening_balance, closing_balance, imported_by, source_filename)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(input.bank_account_id)
    .bind(input.statement_date)
    .bind(input.period_start)
    .bind(input.period_end)
    .bind(input.opening_balance.inner())
    .bind(input.closing_balance.inner())
    .bind(scope.user_id)
    .bind(input.source_filename.as_deref())
    .fetch_one(&mut *tx)
    .await?;

    for (index, line) in input.lines.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO bank_statement_line
                (tenant_id, statement_id, line_no, txn_date, description,
                 reference, withdrawal, deposit)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(scope.tenant_id)
        .bind(statement_id)
        .bind(index as i32 + 1)
        .bind(line.txn_date)
        .bind(line.description.trim())
        .bind(line.reference.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .bind(line.withdrawal.inner())
        .bind(line.deposit.inner())
        .execute(&mut *tx)
        .await
        .with_context(|| format!("row {} of the statement could not be imported", index + 1))?;
    }

    let (matched_exact, matched_by_amount, still_unmatched): (i32, i32, i32) =
        sqlx::query_as("SELECT * FROM match_bank_statement($1)")
            .bind(statement_id)
            .fetch_one(&mut *tx)
            .await
            .context("the statement was imported but could not be matched")?;

    crate::audit(
        &mut tx, scope, actor_label, "import", "bank_statement", Some(statement_id),
        None,
        Some(serde_json::json!({
            "lines": input.lines.len(),
            "matched_exact": matched_exact,
            "matched_by_amount": matched_by_amount,
        })),
        None,
    )
    .await?;

    tx.commit().await?;

    Ok(MatchOutcome { statement_id, matched_exact, matched_by_amount, still_unmatched })
}

// ── Lines and matching ───────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct LineRow {
    id: Uuid,
    line_no: i32,
    txn_date: chrono::NaiveDate,
    description: String,
    reference: Option<String>,
    withdrawal: Decimal,
    deposit: Decimal,
    matched_kind: Option<String>,
    matched_id: Option<Uuid>,
    match_tier: Option<String>,
    matched_reference: Option<String>,
    matched_counterparty: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatementLine {
    pub id: Uuid,
    pub line_no: i32,
    pub txn_date: chrono::NaiveDate,
    pub description: String,
    pub reference: Option<String>,
    pub withdrawal: Money,
    pub deposit: Money,
    pub matched_kind: Option<String>,
    pub matched_id: Option<Uuid>,
    /// Which tier decided this, so a reviewer knows how much to trust it.
    pub match_tier: Option<String>,
    pub matched_reference: Option<String>,
    pub matched_counterparty: Option<String>,
}

pub async fn list_statement_lines(
    pool: &PgPool,
    scope: Scope,
    statement_id: Uuid,
    unmatched_only: bool,
) -> anyhow::Result<Vec<StatementLine>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, LineRow>(
        r#"
        SELECT l.id, l.line_no, l.txn_date, l.description, l.reference,
               l.withdrawal, l.deposit,
               l.matched_kind, l.matched_id, l.match_tier,
               coalesce(d.instrument_no, r.deposit_slip_no) AS matched_reference,
               coalesce(p.name, r.rcd_no)                   AS matched_counterparty
          FROM bank_statement_line l
          LEFT JOIN disbursement d
                 ON l.matched_kind = 'disbursement' AND d.id = l.matched_id
          LEFT JOIN disbursement_voucher dv ON dv.id = d.dv_id
          LEFT JOIN payee p ON p.id = dv.payee_id
          LEFT JOIN report_of_collections r
                 ON l.matched_kind = 'collection' AND r.id = l.matched_id
         WHERE l.statement_id = $1
           AND (NOT $2::boolean OR l.matched_id IS NULL)
         ORDER BY l.line_no
        "#,
    )
    .bind(statement_id)
    .bind(unmatched_only)
    .fetch_all(&mut *tx)
    .await
    .context("could not read the statement")?;

    tx.commit().await?;

    Ok(rows
        .into_iter()
        .map(|r| StatementLine {
            id: r.id,
            line_no: r.line_no,
            txn_date: r.txn_date,
            description: r.description,
            reference: r.reference,
            withdrawal: Money::new(r.withdrawal),
            deposit: Money::new(r.deposit),
            matched_kind: r.matched_kind,
            matched_id: r.matched_id,
            match_tier: r.match_tier,
            matched_reference: r.matched_reference,
            matched_counterparty: r.matched_counterparty,
        })
        .collect())
}

#[derive(Debug, sqlx::FromRow)]
struct SuggestionRow {
    kind: String,
    record_id: Uuid,
    record_ref: Option<String>,
    record_date: chrono::NaiveDate,
    record_amount: Decimal,
    counterparty: Option<String>,
    score: Decimal,
    why: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Suggestion {
    pub kind: String,
    pub record_id: Uuid,
    pub record_ref: Option<String>,
    pub record_date: chrono::NaiveDate,
    pub record_amount: Money,
    pub counterparty: Option<String>,
    pub score: String,
    /// Why this was suggested, in words, so the reviewer is not asked to
    /// trust a number they cannot interpret.
    pub why: Option<String>,
}

pub async fn suggest_matches(
    pool: &PgPool,
    scope: Scope,
    line_id: Uuid,
) -> anyhow::Result<Vec<Suggestion>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, SuggestionRow>(
        "SELECT kind, record_id, record_ref, record_date, record_amount,
                counterparty, score, why
           FROM suggest_statement_matches($1, 5)",
    )
    .bind(line_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not work out any suggestions for this line")?;

    tx.commit().await?;

    Ok(rows
        .into_iter()
        .map(|r| Suggestion {
            kind: r.kind,
            record_id: r.record_id,
            record_ref: r.record_ref,
            record_date: r.record_date,
            record_amount: Money::new(r.record_amount),
            counterparty: r.counterparty,
            score: r.score.to_string(),
            why: r.why,
        })
        .collect())
}

/// Records a match a person made.
///
/// Always tier `manual`, whatever the suggestion score was. Recording an
/// accepted suggestion as `exact` would hide that a human decided it, and the
/// tier is precisely what an auditor asks about.
pub async fn match_line_manually(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    line_id: Uuid,
    kind: &str,
    record_id: Uuid,
) -> anyhow::Result<bool> {
    let mut tx = scoped(pool, scope).await?;

    let updated = sqlx::query(
        r#"
        UPDATE bank_statement_line
           SET matched_kind = $2, matched_id = $3, match_tier = 'manual',
               match_score = NULL, matched_at = now(), matched_by = $4
         WHERE id = $1 AND matched_id IS NULL
        "#,
    )
    .bind(line_id)
    .bind(kind)
    .bind(record_id)
    .bind(scope.user_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated > 0 {
        crate::audit(
            &mut tx, scope, actor_label, "match", "bank_statement_line", Some(line_id),
            None, Some(serde_json::json!({ "kind": kind, "record_id": record_id })), None,
        )
        .await?;
    }

    tx.commit().await?;
    Ok(updated > 0)
}

/// Undoes a match.
///
/// Recorded in the audit trail like any other decision: unmatching is how a
/// mistake gets corrected, and the correction is itself worth a record.
pub async fn unmatch_line(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    line_id: Uuid,
) -> anyhow::Result<bool> {
    let mut tx = scoped(pool, scope).await?;

    let updated = sqlx::query(
        "UPDATE bank_statement_line
            SET matched_kind = NULL, matched_id = NULL, match_tier = NULL,
                match_score = NULL, matched_at = NULL, matched_by = NULL
          WHERE id = $1 AND matched_id IS NOT NULL",
    )
    .bind(line_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated > 0 {
        crate::audit(
            &mut tx, scope, actor_label, "unmatch", "bank_statement_line",
            Some(line_id), None, None, None,
        )
        .await?;
    }

    tx.commit().await?;
    Ok(updated > 0)
}

// ── Reconciliation ───────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct SummaryRow {
    reconciliation_id: Uuid,
    as_of: chrono::NaiveDate,
    status: String,
    book_balance: Decimal,
    bank_balance: Decimal,
    adjusted_book_balance: Decimal,
    adjusted_bank_balance: Decimal,
    difference: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ReconciliationSummary {
    pub reconciliation_id: Uuid,
    pub as_of: chrono::NaiveDate,
    pub status: String,
    pub book_balance: Money,
    pub bank_balance: Money,
    pub adjusted_book_balance: Money,
    pub adjusted_bank_balance: Money,
    /// Zero when the account reconciles. The only number that decides whether
    /// this can be signed off.
    pub difference: Money,
}

pub async fn reconciliation_summary(
    pool: &PgPool,
    scope: Scope,
    reconciliation_id: Uuid,
) -> anyhow::Result<Option<ReconciliationSummary>> {
    let mut tx = scoped(pool, scope).await?;

    let row = sqlx::query_as::<_, SummaryRow>(
        "SELECT reconciliation_id, as_of, status, book_balance, bank_balance,
                adjusted_book_balance, adjusted_bank_balance, difference
           FROM reconciliation_summary WHERE reconciliation_id = $1",
    )
    .bind(reconciliation_id)
    .fetch_optional(&mut *tx)
    .await
    .context("could not read the reconciliation")?;

    tx.commit().await?;

    Ok(row.map(|r| ReconciliationSummary {
        reconciliation_id: r.reconciliation_id,
        as_of: r.as_of,
        status: r.status,
        book_balance: Money::new(r.book_balance),
        bank_balance: Money::new(r.bank_balance),
        adjusted_book_balance: Money::new(r.adjusted_book_balance),
        adjusted_bank_balance: Money::new(r.adjusted_bank_balance),
        difference: Money::new(r.difference),
    }))
}
