//! Budget execution: appropriations, allotments, and obligation requests.
//!
//! The ceilings that matter — an allotment within its appropriation, an
//! obligation within its allotment — are enforced by trigger in migration
//! 0008, under a row lock. Nothing in this module re-implements them; it would
//! be a second, weaker copy of a rule the database already guarantees.
//!
//! Amounts cross this boundary as [`Money`], which serialises as a string.

use anyhow::Context;
use giams_domain::Money;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{scoped, Scope};

// ── Appropriations ───────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct AppropriationRow {
    id: Uuid,
    fund_id: Uuid,
    fund_name: String,
    fiscal_year: i16,
    ordinance_no: String,
    ordinance_date: chrono::NaiveDate,
    appropriation_type: String,
    office_id: Uuid,
    office_name: String,
    account_id: Uuid,
    account_code: String,
    account_name: String,
    expense_class: String,
    amount: Decimal,
    released: Decimal,
    particulars: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Appropriation {
    pub id: Uuid,
    pub fund_id: Uuid,
    pub fund_name: String,
    pub fiscal_year: i16,
    pub ordinance_no: String,
    pub ordinance_date: chrono::NaiveDate,
    pub appropriation_type: String,
    pub office_id: Uuid,
    pub office_name: String,
    pub account_id: Uuid,
    pub account_code: String,
    pub account_name: String,
    pub expense_class: String,
    pub amount: Money,
    pub released_amount: Money,
    /// What may still be released. Computed here so the client never subtracts.
    pub unreleased_amount: Money,
    pub particulars: Option<String>,
}

impl From<AppropriationRow> for Appropriation {
    fn from(r: AppropriationRow) -> Self {
        Appropriation {
            id: r.id,
            fund_id: r.fund_id,
            fund_name: r.fund_name,
            fiscal_year: r.fiscal_year,
            ordinance_no: r.ordinance_no,
            ordinance_date: r.ordinance_date,
            appropriation_type: r.appropriation_type,
            office_id: r.office_id,
            office_name: r.office_name,
            account_id: r.account_id,
            account_code: r.account_code,
            account_name: r.account_name,
            expense_class: r.expense_class,
            amount: Money::new(r.amount),
            released_amount: Money::new(r.released),
            unreleased_amount: Money::new(r.amount - r.released),
            particulars: r.particulars,
        }
    }
}

pub async fn list_appropriations(
    pool: &PgPool,
    scope: Scope,
    fiscal_year: i16,
    fund_id: Option<Uuid>,
) -> anyhow::Result<Vec<Appropriation>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, AppropriationRow>(
        r#"
        SELECT ap.id, ap.fund_id, f.name AS fund_name, ap.fiscal_year,
               ap.ordinance_no, ap.ordinance_date, ap.appropriation_type,
               ap.office_id, o.name AS office_name,
               ap.account_id, a.code AS account_code, a.name AS account_name,
               ap.expense_class::text AS expense_class,
               ap.amount,
               coalesce((SELECT sum(al.amount) FROM allotment al
                          WHERE al.appropriation_id = ap.id), 0) AS released,
               ap.particulars
          FROM appropriation ap
          JOIN fund    f ON f.id = ap.fund_id
          JOIN office  o ON o.id = ap.office_id
          JOIN account a ON a.id = ap.account_id
         WHERE ap.fiscal_year = $1
           AND ($2::uuid IS NULL OR ap.fund_id = $2)
         ORDER BY ap.expense_class, a.code
        "#,
    )
    .bind(fiscal_year)
    .bind(fund_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not read the appropriations")?;

    tx.commit().await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Reads one appropriation with its released and unreleased amounts.
///
/// Shares the projection with [`list_appropriations`] so a row reads the same
/// whether it was just created or listed. Runs inside a caller's transaction so
/// a freshly inserted allotment is already reflected in `released`.
async fn read_appropriation(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> anyhow::Result<Appropriation> {
    let row = sqlx::query_as::<_, AppropriationRow>(
        r#"
        SELECT ap.id, ap.fund_id, f.name AS fund_name, ap.fiscal_year,
               ap.ordinance_no, ap.ordinance_date, ap.appropriation_type,
               ap.office_id, o.name AS office_name,
               ap.account_id, a.code AS account_code, a.name AS account_name,
               ap.expense_class::text AS expense_class,
               ap.amount,
               coalesce((SELECT sum(al.amount) FROM allotment al
                          WHERE al.appropriation_id = ap.id), 0) AS released,
               ap.particulars
          FROM appropriation ap
          JOIN fund    f ON f.id = ap.fund_id
          JOIN office  o ON o.id = ap.office_id
          JOIN account a ON a.id = ap.account_id
         WHERE ap.id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .context("the appropriation could not be read back after saving")?;

    Ok(row.into())
}

/// A new budget line, authorised by a Sanggunian ordinance.
#[derive(Debug, Deserialize)]
pub struct NewAppropriation {
    pub fund_id: Uuid,
    pub fiscal_year: i16,
    pub ordinance_no: String,
    pub ordinance_date: chrono::NaiveDate,
    pub appropriation_type: String,
    pub office_id: Uuid,
    pub account_id: Uuid,
    pub expense_class: String,
    /// Arrives as a string and is stored exactly. Never computed client-side.
    pub amount: Money,
    pub particulars: Option<String>,
}

/// Records one appropriation.
///
/// This is the front door to the whole budget-execution chain: nothing can be
/// released, obligated, or paid until the amount the Sanggunian authorised is
/// on record. The ceiling triggers in migration 0008 measure every later step
/// against the figure written here.
pub async fn create_appropriation(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    input: NewAppropriation,
) -> anyhow::Result<Appropriation> {
    let mut tx = scoped(pool, scope).await?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO appropriation
            (tenant_id, fund_id, fiscal_year, ordinance_no, ordinance_date,
             appropriation_type, office_id, account_id, expense_class, amount,
             particulars, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::expense_class, $10, $11, $12)
        RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(input.fund_id)
    .bind(input.fiscal_year)
    .bind(input.ordinance_no.trim())
    .bind(input.ordinance_date)
    .bind(&input.appropriation_type)
    .bind(input.office_id)
    .bind(input.account_id)
    .bind(&input.expense_class)
    .bind(input.amount.inner())
    .bind(input.particulars.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(scope.user_id)
    .fetch_one(&mut *tx)
    .await?;

    crate::audit(
        &mut tx, scope, actor_label, "create", "appropriation", Some(id),
        None,
        Some(serde_json::json!({
            "ordinance_no": input.ordinance_no,
            "amount": input.amount.to_string(),
        })),
        None,
    )
    .await?;

    let appropriation = read_appropriation(&mut tx, id).await?;
    tx.commit().await?;
    Ok(appropriation)
}

/// A release of part of an appropriation into a spendable allotment.
#[derive(Debug, Deserialize)]
pub struct NewAllotment {
    pub appropriation_id: Uuid,
    pub release_date: chrono::NaiveDate,
    pub amount: Money,
}

/// Releases an allotment against an appropriation.
///
/// The fund, year, expense class, and office are taken from the appropriation,
/// not the caller: a release belongs to the same fund and class as the amount
/// it draws on, and letting the client restate them would allow a release
/// filed under one class to draw down another. The allotment number is
/// allocated gaplessly, and the ceiling trigger refuses a release that exceeds
/// what remains unreleased.
///
/// Returns the appropriation it was released from, so the caller sees the
/// released and unreleased amounts move.
pub async fn create_allotment(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    input: NewAllotment,
) -> anyhow::Result<Appropriation> {
    let mut tx = scoped(pool, scope).await?;

    let (fund_id, fiscal_year, expense_class, office_id): (Uuid, i16, String, Uuid) =
        sqlx::query_as(
            "SELECT fund_id, fiscal_year, expense_class::text, office_id
               FROM appropriation WHERE id = $1",
        )
        .bind(input.appropriation_id)
        .fetch_optional(&mut *tx)
        .await?
        .context("that appropriation could not be found")?;

    let allotment_no: String =
        sqlx::query_scalar("SELECT next_document_number($1, 'ALOT', $2::smallint, NULL, $3)")
            .bind(scope.tenant_id)
            .bind(fiscal_year)
            .bind(fund_id)
            .fetch_one(&mut *tx)
            .await
            .context("could not allocate the next allotment number")?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO allotment
            (tenant_id, appropriation_id, allotment_no, release_date, amount,
             fund_id, fiscal_year, expense_class, office_id, released_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8::expense_class, $9, $10)
        RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(input.appropriation_id)
    .bind(&allotment_no)
    .bind(input.release_date)
    .bind(input.amount.inner())
    .bind(fund_id)
    .bind(fiscal_year)
    .bind(&expense_class)
    .bind(office_id)
    .bind(scope.user_id)
    .fetch_one(&mut *tx)
    .await?;

    crate::audit(
        &mut tx, scope, actor_label, "create", "allotment", Some(id),
        None,
        Some(serde_json::json!({
            "allotment_no": allotment_no,
            "amount": input.amount.to_string(),
        })),
        None,
    )
    .await?;

    let appropriation = read_appropriation(&mut tx, input.appropriation_id).await?;
    tx.commit().await?;
    Ok(appropriation)
}

// ── Obligation requests ──────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct ObligationRow {
    id: Uuid,
    ors_no: String,
    fund_id: Uuid,
    fund_name: String,
    fiscal_year: i16,
    expense_class: String,
    obligation_date: chrono::NaiveDate,
    requesting_office_id: Uuid,
    office_name: String,
    payee_id: Option<Uuid>,
    payee_name: Option<String>,
    particulars: String,
    amount: Decimal,
    payable_amount: Decimal,
    paid_amount: Decimal,
    status: String,
    allotment_id: Uuid,
    allotment_no: String,
    prepared_by_name: Option<String>,
    certified_budget_name: Option<String>,
    certified_accounting_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Obligation {
    pub id: Uuid,
    pub ors_no: String,
    pub fund_id: Uuid,
    pub fund_name: String,
    pub fiscal_year: i16,
    pub expense_class: String,
    pub obligation_date: chrono::NaiveDate,
    pub requesting_office_id: Uuid,
    pub office_name: String,
    pub payee_id: Option<Uuid>,
    pub payee_name: Option<String>,
    pub particulars: String,
    pub amount: Money,
    pub payable_amount: Money,
    pub paid_amount: Money,
    /// Still to be vouchered. The "balance" column of the ORS form.
    pub balance_amount: Money,
    pub status: String,
    pub allotment_id: Uuid,
    pub allotment_no: String,
    /// Names rather than ids: this drives the three signature boxes, and a
    /// blank one is what tells a clerk whose signature is still needed.
    pub prepared_by_name: Option<String>,
    pub certified_budget_name: Option<String>,
    pub certified_accounting_name: Option<String>,
}

impl From<ObligationRow> for Obligation {
    fn from(r: ObligationRow) -> Self {
        Obligation {
            id: r.id,
            ors_no: r.ors_no,
            fund_id: r.fund_id,
            fund_name: r.fund_name,
            fiscal_year: r.fiscal_year,
            expense_class: r.expense_class,
            obligation_date: r.obligation_date,
            requesting_office_id: r.requesting_office_id,
            office_name: r.office_name,
            payee_id: r.payee_id,
            payee_name: r.payee_name,
            particulars: r.particulars,
            amount: Money::new(r.amount),
            payable_amount: Money::new(r.payable_amount),
            paid_amount: Money::new(r.paid_amount),
            balance_amount: Money::new(r.amount - r.payable_amount),
            status: r.status,
            allotment_id: r.allotment_id,
            allotment_no: r.allotment_no,
            prepared_by_name: r.prepared_by_name,
            certified_budget_name: r.certified_budget_name,
            certified_accounting_name: r.certified_accounting_name,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ObligationFilter {
    pub fiscal_year: Option<i16>,
    pub fund_id: Option<Uuid>,
    pub status: Option<String>,
    pub search: Option<String>,
}

pub async fn list_obligations(
    pool: &PgPool,
    scope: Scope,
    filter: ObligationFilter,
) -> anyhow::Result<Vec<Obligation>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, ObligationRow>(
        r#"
        SELECT ob.id, ob.ors_no, ob.fund_id, f.name AS fund_name, ob.fiscal_year,
               ob.expense_class::text AS expense_class, ob.obligation_date,
               ob.requesting_office_id, o.name AS office_name,
               ob.payee_id, p.name AS payee_name,
               ob.particulars, ob.amount, ob.payable_amount, ob.paid_amount,
               ob.status::text AS status,
               ob.allotment_id, al.allotment_no,
               prep.full_name  AS prepared_by_name,
               bud.full_name   AS certified_budget_name,
               acct.full_name  AS certified_accounting_name
          FROM obligation ob
          JOIN fund      f  ON f.id  = ob.fund_id
          JOIN office    o  ON o.id  = ob.requesting_office_id
          JOIN allotment al ON al.id = ob.allotment_id
          LEFT JOIN payee    p    ON p.id    = ob.payee_id
          LEFT JOIN app_user prep ON prep.id = ob.prepared_by
          LEFT JOIN app_user bud  ON bud.id  = ob.certified_budget_by
          LEFT JOIN app_user acct ON acct.id = ob.certified_accounting_by
         WHERE ($1::smallint IS NULL OR ob.fiscal_year = $1)
           AND ($2::uuid     IS NULL OR ob.fund_id     = $2)
           AND ($3::text     IS NULL OR ob.status::text = $3)
           AND ($4::text     IS NULL
                OR ob.ors_no      ILIKE '%' || $4 || '%'
                OR ob.particulars ILIKE '%' || $4 || '%'
                OR p.name         ILIKE '%' || $4 || '%')
         ORDER BY ob.obligation_date DESC, ob.ors_no DESC
         LIMIT 300
        "#,
    )
    .bind(filter.fiscal_year)
    .bind(filter.fund_id)
    .bind(filter.status.as_deref())
    .bind(filter.search.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .fetch_all(&mut *tx)
    .await
    .context("could not read the obligation requests")?;

    tx.commit().await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
pub struct NewObligation {
    pub allotment_id: Uuid,
    pub obligation_date: chrono::NaiveDate,
    pub requesting_office_id: Uuid,
    pub payee_id: Option<Uuid>,
    pub particulars: String,
    /// Arrives as a string and is parsed exactly. The client never computes it.
    pub amount: Money,
}

/// Creates an obligation request in draft.
///
/// The ORS number is allocated inside the same transaction from
/// `document_series`, so the series stays gapless even if this transaction
/// later rolls back — a gap in a voucher series is an audit finding.
pub async fn create_obligation(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    input: NewObligation,
) -> anyhow::Result<Obligation> {
    let mut tx = scoped(pool, scope).await?;

    // The allotment decides the fund, year, and expense class; taking them
    // from the client would let a caller file an obligation against one
    // allotment while reporting it under another class.
    let (fund_id, fiscal_year, expense_class): (Uuid, i16, String) = sqlx::query_as(
        "SELECT fund_id, fiscal_year, expense_class::text FROM allotment WHERE id = $1",
    )
    .bind(input.allotment_id)
    .fetch_optional(&mut *tx)
    .await?
    .context("that allotment could not be found")?;

    let ors_no: String = sqlx::query_scalar(
        "SELECT next_document_number($1, 'ORS', $2::smallint, NULL, $3)",
    )
    .bind(scope.tenant_id)
    .bind(fiscal_year)
    .bind(fund_id)
    .fetch_one(&mut *tx)
    .await
    .context("could not allocate the next obligation request number")?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO obligation
            (tenant_id, ors_no, allotment_id, fund_id, fiscal_year, expense_class,
             obligation_date, requesting_office_id, payee_id, particulars, amount,
             prepared_by, prepared_at, status)
        VALUES ($1, $2, $3, $4, $5, $6::expense_class, $7, $8, $9, $10, $11,
                $12, now(), 'draft')
        RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(&ors_no)
    .bind(input.allotment_id)
    .bind(fund_id)
    .bind(fiscal_year)
    .bind(&expense_class)
    .bind(input.obligation_date)
    .bind(input.requesting_office_id)
    .bind(input.payee_id)
    .bind(input.particulars.trim())
    .bind(input.amount.inner())
    .bind(scope.user_id)
    .fetch_one(&mut *tx)
    .await?;

    crate::audit(
        &mut tx, scope, actor_label, "create", "obligation", Some(id),
        None,
        Some(serde_json::json!({ "ors_no": ors_no, "amount": input.amount.to_string() })),
        None,
    )
    .await?;

    tx.commit().await?;

    // Re-read so the caller gets the joined view rather than a partial echo.
    let mut list = list_obligations(
        pool,
        scope,
        ObligationFilter { search: Some(ors_no.clone()), ..Default::default() },
    )
    .await?;

    list.pop().context("the obligation request could not be read back after saving")
}

/// Which certification box is being signed.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificationBox {
    /// Box B — the Budget Officer certifies the allotment is available.
    Budget,
    /// Box C — the Accountant certifies the obligation is valid.
    Accounting,
}

/// Signs one of the certification boxes.
///
/// Separation of duties is enforced by trigger: the preparer may not certify,
/// and one person may not sign both boxes. Those checks are not repeated here.
///
/// Returns `Ok(None)` when the request does not exist or its preconditions are
/// not met — Box C cannot be signed before Box B. That is an ordinary outcome,
/// not a failure, and returning it as an error would flatten it into the
/// generic "something went wrong" instead of the sentence the clerk needs.
pub async fn certify_obligation(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    id: Uuid,
    which: CertificationBox,
) -> anyhow::Result<Option<Obligation>> {
    let mut tx = scoped(pool, scope).await?;

    let sql = match which {
        CertificationBox::Budget => {
            "UPDATE obligation
                SET certified_budget_by = $2, certified_budget_at = now(),
                    status = CASE WHEN status = 'draft' THEN 'submitted'::document_status
                                  ELSE status END
              WHERE id = $1
              RETURNING ors_no"
        }
        CertificationBox::Accounting => {
            "UPDATE obligation
                SET certified_accounting_by = $2, certified_accounting_at = now(),
                    status = 'certified'::document_status
              WHERE id = $1 AND certified_budget_by IS NOT NULL
              RETURNING ors_no"
        }
    };

    let ors_no: Option<String> = sqlx::query_scalar(sql)
        .bind(id)
        .bind(scope.user_id)
        .fetch_optional(&mut *tx)
        .await?;

    let Some(ors_no) = ors_no else {
        return Ok(None);
    };

    crate::audit(
        &mut tx, scope, actor_label,
        match which {
            CertificationBox::Budget => "certify_budget",
            CertificationBox::Accounting => "certify_accounting",
        },
        "obligation", Some(id), None, None, None,
    )
    .await?;

    tx.commit().await?;

    let mut list = list_obligations(
        pool,
        scope,
        ObligationFilter { search: Some(ors_no), ..Default::default() },
    )
    .await?;

    Ok(list.pop())
}

// ── Registries ───────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct RaodRow {
    allotment_id: Uuid,
    allotment_no: String,
    release_date: chrono::NaiveDate,
    expense_class: String,
    fund_name: String,
    office_name: String,
    allotment_amount: Decimal,
    obligation_amount: Decimal,
    disbursement_amount: Decimal,
    unobligated_balance: Decimal,
}

#[derive(Debug, Serialize)]
pub struct RaodEntry {
    pub allotment_id: Uuid,
    pub allotment_no: String,
    pub release_date: chrono::NaiveDate,
    pub expense_class: String,
    pub fund_name: String,
    pub office_name: String,
    pub allotment_amount: Money,
    pub obligation_amount: Money,
    pub disbursement_amount: Money,
    pub unobligated_balance: Money,
}

/// The Registry of Allotments, Obligations and Disbursements.
///
/// COA expects one registry per expense class; pass `expense_class` to get
/// RAOD-PS, RAOD-MOOE, RAOD-FE, or RAOD-CO.
pub async fn registry_raod(
    pool: &PgPool,
    scope: Scope,
    fiscal_year: i16,
    fund_id: Option<Uuid>,
    expense_class: Option<String>,
) -> anyhow::Result<Vec<RaodEntry>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, RaodRow>(
        r#"
        SELECT r.allotment_id, r.allotment_no, r.release_date,
               r.expense_class::text AS expense_class,
               f.name AS fund_name, o.name AS office_name,
               r.allotment_amount, r.obligation_amount,
               r.disbursement_amount, r.unobligated_balance
          FROM registry_raod r
          JOIN fund   f ON f.id = r.fund_id
          JOIN office o ON o.id = r.office_id
         WHERE r.fiscal_year = $1
           AND ($2::uuid IS NULL OR r.fund_id = $2)
           AND ($3::text IS NULL OR r.expense_class::text = $3)
         ORDER BY r.expense_class, r.release_date, r.allotment_no
        "#,
    )
    .bind(fiscal_year)
    .bind(fund_id)
    .bind(expense_class.as_deref())
    .fetch_all(&mut *tx)
    .await
    .context("could not read the registry of allotments and obligations")?;

    tx.commit().await?;

    Ok(rows
        .into_iter()
        .map(|r| RaodEntry {
            allotment_id: r.allotment_id,
            allotment_no: r.allotment_no,
            release_date: r.release_date,
            expense_class: r.expense_class,
            fund_name: r.fund_name,
            office_name: r.office_name,
            allotment_amount: Money::new(r.allotment_amount),
            obligation_amount: Money::new(r.obligation_amount),
            disbursement_amount: Money::new(r.disbursement_amount),
            unobligated_balance: Money::new(r.unobligated_balance),
        })
        .collect())
}

/// Allotments with room left, for the obligation form's picker.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AllotmentOption {
    pub id: Uuid,
    pub allotment_no: String,
    pub expense_class: String,
    pub fund_name: String,
    pub office_name: String,
    #[sqlx(rename = "available")]
    #[serde(serialize_with = "serialize_decimal_as_money")]
    pub available_amount: Decimal,
}

fn serialize_decimal_as_money<S: serde::Serializer>(
    value: &Decimal,
    s: S,
) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&Money::new(*value), s)
}

pub async fn list_allotment_options(
    pool: &PgPool,
    scope: Scope,
    fiscal_year: i16,
) -> anyhow::Result<Vec<AllotmentOption>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, AllotmentOption>(
        r#"
        SELECT r.allotment_id AS id, r.allotment_no,
               r.expense_class::text AS expense_class,
               f.name AS fund_name, o.name AS office_name,
               r.unobligated_balance AS available
          FROM registry_raod r
          JOIN fund   f ON f.id = r.fund_id
          JOIN office o ON o.id = r.office_id
         WHERE r.fiscal_year = $1
           AND r.unobligated_balance > 0
         ORDER BY r.expense_class, r.allotment_no
        "#,
    )
    .bind(fiscal_year)
    .fetch_all(&mut *tx)
    .await
    .context("could not read the available allotments")?;

    tx.commit().await?;
    Ok(rows)
}

