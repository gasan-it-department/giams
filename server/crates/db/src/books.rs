//! The books of accounts, as the Accounting Office keeps them.
//!
//! Six forms, taken from the office's own workbook:
//!
//! | Annex | Form                          | Journal type |
//! |-------|-------------------------------|--------------|
//! | 1     | General Journal               | `GJ`         |
//! | 2     | Cash Receipts Journal         | `CRJ`        |
//! | 3     | Check Disbursements Journal   | `CkDJ`       |
//! | 4     | Cash Disbursements Journal    | `CDJ`        |
//! | 23    | Journal Entry Voucher         | —            |
//! | —     | Trial Balance                 | —            |
//!
//! The four journals are the same query with a different `journal_type`, which
//! is why the ledger modelled that column from the start rather than inferring
//! the book from what an entry touched. Every one of them reads only *posted*
//! entries: a book of accounts records what happened, not what was proposed.

use anyhow::Context;
use giams_domain::Money;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{scoped, Refusal, Scope};

/// Which book to read.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Book {
    /// Annex 1 — adjusting, closing, and non-cash entries.
    Gj,
    /// Annex 2 — collections.
    Crj,
    /// Annex 3 — payments by cheque.
    #[serde(rename = "CKDJ")]
    CkDj,
    /// Annex 4 — payments in cash and by ADA.
    Cdj,
}

impl Book {
    pub fn journal_code(self) -> &'static str {
        self.journal_type()
    }

    fn journal_type(self) -> &'static str {
        match self {
            Book::Gj => "GJ",
            Book::Crj => "CRJ",
            Book::CkDj => "CkDJ",
            Book::Cdj => "CDJ",
        }
    }

    pub fn annex(self) -> &'static str {
        match self {
            Book::Gj => "Annex 1",
            Book::Crj => "Annex 2",
            Book::CkDj => "Annex 3",
            Book::Cdj => "Annex 4",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Book::Gj => "GENERAL JOURNAL",
            Book::Crj => "CASH RECEIPTS JOURNAL",
            Book::CkDj => "CHECK DISBURSEMENTS JOURNAL",
            Book::Cdj => "CASH DISBURSEMENTS JOURNAL",
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct BookLineRow {
    entry_date: chrono::NaiveDate,
    jev_no: String,
    particulars: String,
    reference: Option<String>,
    counterparty: Option<String>,
    account_code: String,
    account_name: String,
    debit: Decimal,
    credit: Decimal,
}

/// One line of a book of accounts.
///
/// Deliberately one row per *journal line*, not per entry: that is how the
/// paper form is ruled, and an accountant checking a book against it reads
/// down the debit and credit columns.
#[derive(Debug, Serialize)]
pub struct BookLine {
    pub entry_date: chrono::NaiveDate,
    pub jev_no: String,
    pub particulars: String,
    /// Cheque number for the CkDJ, RCD number for the CRJ.
    pub reference: Option<String>,
    /// Payee on a disbursement, collecting officer on a receipt.
    pub counterparty: Option<String>,
    pub account_code: String,
    pub account_name: String,
    pub debit: Money,
    pub credit: Money,
}

#[derive(Debug, Serialize)]
pub struct BookPage {
    pub annex: String,
    pub title: String,
    pub municipality: String,
    pub fund_name: String,
    pub period_label: String,
    pub lines: Vec<BookLine>,
    pub total_debit: Money,
    pub total_credit: Money,
}

pub async fn read_book(
    pool: &PgPool,
    scope: Scope,
    book: Book,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    fund_id: Option<Uuid>,
) -> anyhow::Result<BookPage> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, BookLineRow>(
        r#"
        SELECT je.entry_date, je.jev_no, je.particulars,
               -- The reference a clerk looks the line up by differs per book.
               coalesce(d.instrument_no, r.rcd_no)  AS reference,
               coalesce(p.name, ofc.name)           AS counterparty,
               a.code AS account_code, a.name AS account_name,
               l.debit, l.credit
          FROM journal_entry je
          JOIN journal_entry_line l ON l.entry_id = je.id
          JOIN account a            ON a.id = l.account_id
          LEFT JOIN payee  p   ON p.id   = l.payee_id
          LEFT JOIN office ofc ON ofc.id = l.office_id
          LEFT JOIN disbursement d
                 ON je.source_type = 'DV' AND d.id = je.source_id
          LEFT JOIN report_of_collections r
                 ON je.source_type = 'OR' AND r.id = je.source_id
         WHERE je.tenant_id = $1
           AND je.status    = 'posted'
           AND je.journal_type = $2::journal_type
           AND je.entry_date BETWEEN $3 AND $4
           AND ($5::uuid IS NULL OR je.fund_id = $5)
         ORDER BY je.entry_date, je.jev_no, l.line_no
        "#,
    )
    .bind(scope.tenant_id)
    .bind(book.journal_type())
    .bind(from)
    .bind(to)
    .bind(fund_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not read the book of accounts")?;

    let (municipality, fund_name) = heading(&mut tx, scope, fund_id).await?;

    // Totals summed in exact decimal here, never in the client.
    let total_debit: Decimal = rows.iter().map(|r| r.debit).sum();
    let total_credit: Decimal = rows.iter().map(|r| r.credit).sum();

    tx.commit().await?;

    Ok(BookPage {
        annex: book.annex().to_string(),
        title: book.title().to_string(),
        municipality,
        fund_name,
        period_label: format!("{from} to {to}"),
        lines: rows
            .into_iter()
            .map(|r| BookLine {
                entry_date: r.entry_date,
                jev_no: r.jev_no,
                particulars: r.particulars,
                reference: r.reference,
                counterparty: r.counterparty,
                account_code: r.account_code,
                account_name: r.account_name,
                debit: Money::new(r.debit),
                credit: Money::new(r.credit),
            })
            .collect(),
        total_debit: Money::new(total_debit),
        total_credit: Money::new(total_credit),
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

// ── Trial balance ────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct TrialBalanceRow {
    account_code: String,
    account_name: String,
    account_type: String,
    debit_total: Decimal,
    credit_total: Decimal,
    balance: Decimal,
    normal_balance: String,
}

#[derive(Debug, Serialize)]
pub struct TrialBalanceLine {
    pub account_code: String,
    pub account_name: String,
    pub account_type: String,
    /// The office's form has two columns; an account appears in one of them.
    pub debit_balance: Money,
    pub credit_balance: Money,
}

#[derive(Debug, Serialize)]
pub struct TrialBalancePage {
    pub municipality: String,
    pub fund_name: String,
    pub as_of: chrono::NaiveDate,
    pub lines: Vec<TrialBalanceLine>,
    pub total_debit: Money,
    pub total_credit: Money,
    /// Zero when the ledger balances. Anything else means it must not be filed.
    pub difference: Money,
}

pub async fn trial_balance(
    pool: &PgPool,
    scope: Scope,
    as_of: chrono::NaiveDate,
    fund_id: Option<Uuid>,
) -> anyhow::Result<TrialBalancePage> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, TrialBalanceRow>(
        "SELECT account_code, account_name, account_type,
                debit_total, credit_total, balance, normal_balance
           FROM trial_balance_as_of($1, $2, $3)",
    )
    .bind(scope.tenant_id)
    .bind(as_of)
    .bind(fund_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not produce the trial balance")?;

    let (municipality, fund_name) = heading(&mut tx, scope, fund_id).await?;
    tx.commit().await?;

    let mut total_debit = Decimal::ZERO;
    let mut total_credit = Decimal::ZERO;

    let lines = rows
        .into_iter()
        .map(|r| {
            // An account lands in the column of its normal side, unless its
            // balance has gone the other way — which happens, and hiding it
            // would be the wrong kind of tidy.
            let debit_side = (r.normal_balance == "debit") == (r.balance >= Decimal::ZERO);
            let magnitude = r.balance.abs();

            if debit_side {
                total_debit += magnitude;
            } else {
                total_credit += magnitude;
            }

            TrialBalanceLine {
                account_code: r.account_code,
                account_name: r.account_name,
                account_type: r.account_type,
                debit_balance: Money::new(if debit_side { magnitude } else { Decimal::ZERO }),
                credit_balance: Money::new(if debit_side { Decimal::ZERO } else { magnitude }),
            }
        })
        .collect();

    Ok(TrialBalancePage {
        municipality,
        fund_name,
        as_of,
        lines,
        total_debit: Money::new(total_debit),
        total_credit: Money::new(total_credit),
        difference: Money::new(total_debit - total_credit),
    })
}

// ── Writing a journal entry voucher ──────────────────────────────────────────

/// One accounting line of a JEV.
///
/// A line carries an amount in exactly one column, which the database also
/// enforces. Both are sent because the form has two columns and rejecting the
/// nonsense case with a clear message beats silently picking one.
#[derive(Debug, Deserialize)]
pub struct NewEntryLine {
    pub account_id: Uuid,
    pub debit: Money,
    pub credit: Money,
    pub particulars: Option<String>,
    pub payee_id: Option<Uuid>,
    pub office_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct NewJournalEntry {
    pub fund_id: Uuid,
    pub entry_date: chrono::NaiveDate,
    pub particulars: String,
    /// Which book it belongs in: GJ, CRJ, CkDJ, CDJ.
    pub journal_type: String,
    pub lines: Vec<NewEntryLine>,
}

/// Creates a journal entry voucher in draft.
///
/// Draft deliberately: the balanced-entry constraint binds from submission on,
/// so a clerk can save half-finished work and come back to it. Posting is a
/// separate, permissioned step.
///
/// The JEV number is allocated inside the transaction from `document_series`,
/// so a rollback consumes no number and the series stays gapless.
pub async fn create_journal_entry(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    input: NewJournalEntry,
) -> anyhow::Result<(Uuid, String)> {
    let mut tx = scoped(pool, scope).await?;

    let fiscal_year = {
        use chrono::Datelike;
        input.entry_date.year() as i16
    };
    let month = {
        use chrono::Datelike;
        input.entry_date.month() as i16
    };

    let period_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM posting_period
          WHERE fund_id = $1 AND $2::date BETWEEN starts_on AND ends_on",
    )
    .bind(input.fund_id)
    .bind(input.entry_date)
    .fetch_optional(&mut *tx)
    .await?
    // Not a fault. Somebody picked a date in a month nobody has opened yet,
    // which is an ordinary thing to do in January or on the first of a month,
    // and the answer is to open the period — not to show a reference number.
    .ok_or_else(|| {
        Refusal::with_hint(
            "PERIOD_NOT_OPEN",
            format!(
                "No accounting period has been opened for {} for this fund, \
                 so nothing can be recorded on {}.",
                input.entry_date.format("%B %Y"),
                input.entry_date.format("%-d %B %Y"),
            ),
            "Ask the Municipal Accountant to open that month, or change the date \
             to one inside a month that is already open.",
        )
    })?;

    let jev_no: String =
        sqlx::query_scalar("SELECT next_document_number($1, 'JEV', $2::smallint, $3::smallint, $4)")
            .bind(scope.tenant_id)
            .bind(fiscal_year)
            .bind(month)
            .bind(input.fund_id)
            .fetch_one(&mut *tx)
            .await
            .context("could not allocate the next journal entry number")?;

    let entry_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO journal_entry
            (tenant_id, jev_no, journal_type, fund_id, period_id, entry_date,
             particulars, status, source_type, prepared_by, prepared_at)
        VALUES ($1, $2, $3::journal_type, $4, $5, $6, $7, 'draft', 'manual', $8, now())
        RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(&jev_no)
    .bind(&input.journal_type)
    .bind(input.fund_id)
    .bind(period_id)
    .bind(input.entry_date)
    .bind(input.particulars.trim())
    .bind(scope.user_id)
    .fetch_one(&mut *tx)
    .await?;

    for (index, line) in input.lines.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO journal_entry_line
                (tenant_id, entry_id, line_no, account_id, debit, credit,
                 particulars, payee_id, office_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(scope.tenant_id)
        .bind(entry_id)
        .bind(index as i16 + 1)
        .bind(line.account_id)
        .bind(line.debit.inner())
        .bind(line.credit.inner())
        .bind(line.particulars.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .bind(line.payee_id)
        .bind(line.office_id)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("line {} could not be saved", index + 1))?;
    }

    crate::audit(
        &mut tx, scope, actor_label, "create", "journal_entry", Some(entry_id),
        None, Some(serde_json::json!({ "jev_no": jev_no, "lines": input.lines.len() })), None,
    )
    .await?;

    tx.commit().await?;
    Ok((entry_id, jev_no))
}

/// Submits a draft entry and posts it to the general ledger.
///
/// One step because they are one decision for a manual JEV: an accountant with
/// `jev.post` who has finished typing an entry means it to be in the books.
/// The balanced-entry constraint fires at COMMIT, so an unbalanced entry is
/// refused here with the difference named.
pub async fn post_journal_entry(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    entry_id: Uuid,
) -> anyhow::Result<Option<String>> {
    let mut tx = scoped(pool, scope).await?;

    let jev_no: Option<String> = sqlx::query_scalar(
        "UPDATE journal_entry
            SET status = 'posted', posted_by = $2, posted_at = now()
          WHERE id = $1 AND status = 'draft'
          RETURNING jev_no",
    )
    .bind(entry_id)
    .bind(scope.user_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(jev_no) = jev_no else {
        return Ok(None);
    };

    crate::audit(
        &mut tx, scope, actor_label, "post", "journal_entry", Some(entry_id),
        None, Some(serde_json::json!({ "jev_no": jev_no })), None,
    )
    .await?;

    // Commits here: the deferred balance check runs now, and its message is
    // what the caller surfaces if the entry does not balance.
    tx.commit().await?;
    Ok(Some(jev_no))
}
