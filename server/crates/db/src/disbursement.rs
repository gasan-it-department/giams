//! Disbursement vouchers, their deductions, and payments.
//!
//! §4.1 requires gross, deductions, and net to be computed **on the server**.
//! That is not only a security position — it is what makes the figures
//! reproducible. The client sends a gross amount and which deductions apply;
//! the rates in force on the voucher date are resolved here, the arithmetic
//! happens here in exact decimal, and the resolved rate is stored on the line
//! so reprinting the voucher years later reproduces it.

use anyhow::Context;
use giams_domain::Money;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{scoped, Scope};

// ── Reading ──────────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct VoucherRow {
    id: Uuid,
    dv_no: String,
    fund_name: String,
    fiscal_year: i16,
    dv_date: chrono::NaiveDate,
    particulars: String,
    gross_amount: Decimal,
    total_deductions: Decimal,
    net_amount: Decimal,
    status: String,
    payee_name: String,
    office_name: String,
    ors_no: String,
    obligation_id: Uuid,
    prepared_by_name: Option<String>,
    approved_by_name: Option<String>,
    paid_amount: Decimal,
    posted: bool,
}

#[derive(Debug, Serialize)]
pub struct Voucher {
    pub id: Uuid,
    pub dv_no: String,
    pub fund_name: String,
    pub fiscal_year: i16,
    pub dv_date: chrono::NaiveDate,
    pub particulars: String,
    pub gross_amount: Money,
    pub total_deductions: Money,
    pub net_amount: Money,
    /// How much of the net has actually been paid out.
    pub paid_amount: Money,
    pub status: String,
    pub payee_name: String,
    pub office_name: String,
    pub ors_no: String,
    pub obligation_id: Uuid,
    pub prepared_by_name: Option<String>,
    pub approved_by_name: Option<String>,
    /// Whether the accrual entry has reached the general ledger.
    pub posted: bool,
}

impl From<VoucherRow> for Voucher {
    fn from(r: VoucherRow) -> Self {
        Voucher {
            id: r.id,
            dv_no: r.dv_no,
            fund_name: r.fund_name,
            fiscal_year: r.fiscal_year,
            dv_date: r.dv_date,
            particulars: r.particulars,
            gross_amount: Money::new(r.gross_amount),
            total_deductions: Money::new(r.total_deductions),
            net_amount: Money::new(r.net_amount),
            paid_amount: Money::new(r.paid_amount),
            status: r.status,
            payee_name: r.payee_name,
            office_name: r.office_name,
            ors_no: r.ors_no,
            obligation_id: r.obligation_id,
            prepared_by_name: r.prepared_by_name,
            approved_by_name: r.approved_by_name,
            posted: r.posted,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct VoucherFilter {
    pub fiscal_year: Option<i16>,
    pub status: Option<String>,
    pub search: Option<String>,
}

pub async fn list_vouchers(
    pool: &PgPool,
    scope: Scope,
    filter: VoucherFilter,
) -> anyhow::Result<Vec<Voucher>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, VoucherRow>(
        r#"
        SELECT dv.id, dv.dv_no, f.name AS fund_name, dv.fiscal_year, dv.dv_date,
               dv.particulars, dv.gross_amount, dv.total_deductions, dv.net_amount,
               dv.status::text AS status,
               p.name AS payee_name, o.name AS office_name,
               ob.ors_no, dv.obligation_id,
               prep.full_name AS prepared_by_name,
               appr.full_name AS approved_by_name,
               coalesce((SELECT sum(d.amount) FROM disbursement d
                          WHERE d.dv_id = dv.id AND d.status = 'issued'), 0) AS paid_amount,
               EXISTS (SELECT 1 FROM journal_entry je
                        WHERE je.source_type = 'DV' AND je.source_id = dv.id
                          AND je.status = 'posted') AS posted
          FROM disbursement_voucher dv
          JOIN fund       f  ON f.id  = dv.fund_id
          JOIN payee      p  ON p.id  = dv.payee_id
          JOIN office     o  ON o.id  = dv.office_id
          JOIN obligation ob ON ob.id = dv.obligation_id
          LEFT JOIN app_user prep ON prep.id = dv.prepared_by
          LEFT JOIN app_user appr ON appr.id = dv.approved_by
         WHERE ($1::smallint IS NULL OR dv.fiscal_year  = $1)
           AND ($2::text     IS NULL OR dv.status::text = $2)
           AND ($3::text     IS NULL
                OR dv.dv_no       ILIKE '%' || $3 || '%'
                OR dv.particulars ILIKE '%' || $3 || '%'
                OR p.name         ILIKE '%' || $3 || '%')
         ORDER BY dv.dv_date DESC, dv.dv_no DESC
         LIMIT 300
        "#,
    )
    .bind(filter.fiscal_year)
    .bind(filter.status.as_deref())
    .bind(filter.search.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .fetch_all(&mut *tx)
    .await
    .context("could not read the voucher register")?;

    tx.commit().await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct VoucherDeduction {
    pub code: String,
    pub name: String,
    pub remit_to: String,
    #[sqlx(rename = "base_amount")]
    #[serde(serialize_with = "as_money")]
    pub base: Decimal,
    pub rate_applied: Option<Decimal>,
    #[sqlx(rename = "amount")]
    #[serde(serialize_with = "as_money")]
    pub amount_deducted: Decimal,
    pub authority: Option<String>,
}

fn as_money<S: serde::Serializer>(value: &Decimal, s: S) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&Money::new(*value), s)
}

pub async fn voucher_deductions(
    pool: &PgPool,
    scope: Scope,
    dv_id: Uuid,
) -> anyhow::Result<Vec<VoucherDeduction>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, VoucherDeduction>(
        r#"
        SELECT dt.code, dt.name, dt.remit_to,
               dd.base_amount, dd.rate_applied, dd.amount, dd.authority
          FROM dv_deduction dd
          JOIN deduction_type dt ON dt.id = dd.deduction_type_id
         WHERE dd.dv_id = $1
         ORDER BY dt.code
        "#,
    )
    .bind(dv_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not read the deductions on this voucher")?;

    tx.commit().await?;
    Ok(rows)
}

/// The deductions a clerk can choose from, for a given kind of payee.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DeductionOption {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub remit_to: String,
    pub basis: String,
}

pub async fn list_deduction_options(
    pool: &PgPool,
    scope: Scope,
    applies_to: Option<String>,
) -> anyhow::Result<Vec<DeductionOption>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, DeductionOption>(
        r#"
        SELECT id, code, name, remit_to, basis
          FROM deduction_type
         WHERE is_active
           AND ($1::text IS NULL OR applies_to = $1 OR applies_to = 'both')
         ORDER BY code
        "#,
    )
    .bind(applies_to.as_deref())
    .fetch_all(&mut *tx)
    .await
    .context("could not read the list of deductions")?;

    tx.commit().await?;
    Ok(rows)
}

// ── Writing ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NewVoucher {
    pub obligation_id: Uuid,
    pub dv_date: chrono::NaiveDate,
    pub particulars: String,
    pub gross_amount: Money,
    /// Which deductions to apply. Rates are resolved here, not sent by the
    /// client — a client that could choose the rate could choose the tax.
    #[serde(default)]
    pub deduction_type_ids: Vec<Uuid>,
}

/// Creates a voucher and computes its deductions.
///
/// Everything happens in one transaction: the voucher number is allocated, the
/// deduction lines are computed from the rates in force on `dv_date`, and the
/// voucher total is refreshed by trigger. If any part fails, no number is
/// consumed and no partial voucher is left behind.
pub async fn create_voucher(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    input: NewVoucher,
) -> anyhow::Result<Uuid> {
    let mut tx = scoped(pool, scope).await?;

    // The obligation decides fund, year, office, and payee. Taking them from
    // the client would let a voucher be charged to one obligation while
    // reporting against another.
    let (fund_id, fiscal_year, office_id, payee_id): (Uuid, i16, Uuid, Option<Uuid>) =
        sqlx::query_as(
            "SELECT fund_id, fiscal_year, requesting_office_id, payee_id
               FROM obligation WHERE id = $1",
        )
        .bind(input.obligation_id)
        .fetch_optional(&mut *tx)
        .await?
        .context("that obligation request could not be found")?;

    let payee_id = payee_id.context(
        "that obligation request has no payee recorded, so a voucher cannot be raised against it",
    )?;

    // The period containing the voucher date. Posting into a closed period is
    // refused downstream, but resolving it here gives a clearer failure.
    let period_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM posting_period
          WHERE fund_id = $1 AND $2::date BETWEEN starts_on AND ends_on",
    )
    .bind(fund_id)
    .bind(input.dv_date)
    .fetch_optional(&mut *tx)
    .await?
    .context("there is no accounting period covering that voucher date")?;

    let month = {
        use chrono::Datelike;
        input.dv_date.month() as i16
    };

    let dv_no: String = sqlx::query_scalar(
        "SELECT next_document_number($1, 'DV', $2::smallint, $3::smallint, $4)",
    )
    .bind(scope.tenant_id)
    .bind(fiscal_year)
    .bind(month)
    .bind(fund_id)
    .fetch_one(&mut *tx)
    .await
    .context("could not allocate the next voucher number")?;

    let dv_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO disbursement_voucher
            (tenant_id, dv_no, fund_id, fiscal_year, period_id, obligation_id,
             payee_id, office_id, dv_date, particulars, gross_amount,
             prepared_by, prepared_at, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now(), 'draft')
        RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(&dv_no)
    .bind(fund_id)
    .bind(fiscal_year)
    .bind(period_id)
    .bind(input.obligation_id)
    .bind(payee_id)
    .bind(office_id)
    .bind(input.dv_date)
    .bind(input.particulars.trim())
    .bind(input.gross_amount.inner())
    .bind(scope.user_id)
    .fetch_one(&mut *tx)
    .await?;

    // Compute each deduction from the rate in force on the voucher date, and
    // store what was resolved alongside the amount.
    for type_id in &input.deduction_type_ids {
        sqlx::query(
            r#"
            INSERT INTO dv_deduction
                (tenant_id, dv_id, deduction_type_id, base_amount,
                 rate_applied, fixed_applied, rate_id, authority, amount)
            SELECT $1, $2, $3, $4,
                   r.rate, r.fixed_amount, r.rate_id, r.authority,
                   to_centavo(coalesce(r.fixed_amount, $4 * r.rate))
              FROM resolve_deduction_rate($3, $5, $4) r
            "#,
        )
        .bind(scope.tenant_id)
        .bind(dv_id)
        .bind(type_id)
        .bind(input.gross_amount.inner())
        .bind(input.dv_date)
        .execute(&mut *tx)
        .await
        .context("could not work out one of the deductions")?;
    }

    crate::audit(
        &mut tx, scope, actor_label, "create", "disbursement_voucher", Some(dv_id),
        None,
        Some(serde_json::json!({ "dv_no": dv_no, "gross": input.gross_amount.to_string() })),
        None,
    )
    .await?;

    tx.commit().await?;
    Ok(dv_id)
}

/// Moves a voucher through its approval steps.
///
/// Returns `Ok(None)` when the transition does not apply — the voucher is
/// missing, or is not in a state the step can act on. That is an ordinary
/// outcome for a stale screen, and the caller phrases it.
pub async fn advance_voucher(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    dv_id: Uuid,
    step: VoucherStep,
) -> anyhow::Result<Option<String>> {
    let mut tx = scoped(pool, scope).await?;

    let sql = match step {
        VoucherStep::Submit => {
            "UPDATE disbursement_voucher SET status = 'submitted'
              WHERE id = $1 AND status = 'draft' RETURNING dv_no"
        }
        VoucherStep::Certify => {
            "UPDATE disbursement_voucher
                SET status = 'certified', certified_by = $2, certified_at = now()
              WHERE id = $1 AND status = 'submitted' RETURNING dv_no"
        }
        VoucherStep::Approve => {
            "UPDATE disbursement_voucher
                SET status = 'approved', approved_by = $2, approved_at = now()
              WHERE id = $1 AND status = 'certified' RETURNING dv_no"
        }
    };

    let dv_no: Option<String> = sqlx::query_scalar(sql)
        .bind(dv_id)
        .bind(scope.user_id)
        .fetch_optional(&mut *tx)
        .await?;

    if let Some(ref no) = dv_no {
        crate::audit(
            &mut tx, scope, actor_label, step.action(), "disbursement_voucher",
            Some(dv_id), None, Some(serde_json::json!({ "dv_no": no })), None,
        )
        .await?;
    }

    tx.commit().await?;
    Ok(dv_no)
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoucherStep {
    Submit,
    Certify,
    Approve,
}

impl VoucherStep {
    fn action(self) -> &'static str {
        match self {
            VoucherStep::Submit => "submit",
            VoucherStep::Certify => "certify",
            VoucherStep::Approve => "approve",
        }
    }
}

/// Posts the accrual entry for an approved voucher.
///
/// The work is done by `post_voucher_accrual` in the database, which builds
/// the balanced entry and is idempotent. Calling it here keeps the ledger
/// arithmetic in one place rather than reimplementing it in Rust.
pub async fn post_voucher(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    dv_id: Uuid,
) -> anyhow::Result<Uuid> {
    let mut tx = scoped(pool, scope).await?;

    let entry_id: Uuid = sqlx::query_scalar("SELECT post_voucher_accrual($1, $2)")
        .bind(dv_id)
        .bind(scope.user_id)
        .fetch_one(&mut *tx)
        .await?;

    crate::audit(
        &mut tx, scope, actor_label, "post", "disbursement_voucher", Some(dv_id),
        None, Some(serde_json::json!({ "journal_entry_id": entry_id })), None,
    )
    .await?;

    tx.commit().await?;
    Ok(entry_id)
}
