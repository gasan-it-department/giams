//! Funds and the chart of accounts.
//!
//! Every query here runs through a tenant-scoped transaction, so row level
//! security applies. None of these functions take a tenant id as an argument:
//! it comes from the [`Scope`], which is what stops a caller passing the wrong
//! one.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{scoped, Scope};

// ── Funds ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Fund {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub parent_fund_id: Option<Uuid>,
    pub is_statutory_subfund: bool,
    pub is_active: bool,
}

pub async fn list_funds(pool: &PgPool, scope: Scope) -> anyhow::Result<Vec<Fund>> {
    let mut tx = scoped(pool, scope).await?;

    let funds = sqlx::query_as::<_, Fund>(
        r#"
        SELECT id, code, name, kind::text AS kind, parent_fund_id,
               is_statutory_subfund, is_active
          FROM fund
         ORDER BY code
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .context("could not read the list of funds")?;

    tx.commit().await?;
    Ok(funds)
}

// ── Offices ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Office {
    pub id: Uuid,
    pub code: String,
    pub name: String,
}

/// The offices an appropriation can be made to and an obligation charged
/// against. Only the active ones: an office closed or merged by ordinance
/// should not appear on a form for a new budget line.
pub async fn list_offices(pool: &PgPool, scope: Scope) -> anyhow::Result<Vec<Office>> {
    let mut tx = scoped(pool, scope).await?;

    let offices = sqlx::query_as::<_, Office>(
        r#"
        SELECT id, code, name
          FROM office
         WHERE is_active
         ORDER BY code
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .context("could not read the list of offices")?;

    tx.commit().await?;
    Ok(offices)
}

// ── Chart of accounts ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Account {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub account_type: String,
    /// Derived in the database from the account type and the contra flag,
    /// never hand-entered.
    pub normal_balance: String,
    pub parent_id: Option<Uuid>,
    pub is_postable: bool,
    pub is_contra: bool,
    pub requires_subsidiary: bool,
    pub expense_class: Option<String>,
    pub is_cash: bool,
    pub is_active: bool,
    /// What the account is for, from Annex B of the GAM.
    pub description: Option<String>,
}

/// A page of results, with the total behind it.
///
/// The total is what lets the toolbar say "1-20 of 263" rather than just
/// showing arrows and hoping. Counted in the same query as the page so the two
/// can never disagree.
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// Rows per page.
///
/// Twenty: enough that a clerk rarely pages, few enough that the list never
/// takes a moment to draw on the modest hardware these offices run.
pub const PAGE_SIZE: i64 = 20;

#[derive(Debug, Deserialize)]
pub struct AccountFilter {
    /// Free text over code and name.
    pub search: Option<String>,
    pub account_type: Option<String>,
    /// When true, only accounts that may appear on a journal entry line.
    pub postable_only: Option<bool>,
    pub include_inactive: Option<bool>,
    /// Zero-based.
    pub page: Option<i64>,
}

pub async fn list_accounts(
    pool: &PgPool,
    scope: Scope,
    filter: AccountFilter,
) -> anyhow::Result<Page<Account>> {
    let mut tx = scoped(pool, scope).await?;

    let offset = filter.page.unwrap_or(0).max(0) * PAGE_SIZE;
    let search = filter.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let postable = filter.postable_only.unwrap_or(false);
    let inactive = filter.include_inactive.unwrap_or(false);

    // Counted before the page is taken, with exactly the same predicates. A
    // total derived from different conditions than the rows is worse than no
    // total: it makes the pager lie.
    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
          FROM account
         WHERE ($1::text IS NULL OR code ILIKE '%' || $1 || '%' OR name ILIKE '%' || $1 || '%')
           AND ($2::text IS NULL OR account_type::text = $2)
           AND (NOT $3::boolean OR is_postable)
           AND ($4::boolean OR is_active)
        "#,
    )
    .bind(search)
    .bind(filter.account_type.as_deref())
    .bind(postable)
    .bind(inactive)
    .fetch_one(&mut *tx)
    .await
    .context("could not count the chart of accounts")?;

    // Bound parameters throughout — no fragment of this is built by
    // concatenation, so a search term can never become SQL.
    let items = sqlx::query_as::<_, Account>(
        r#"
        SELECT id, code, name,
               account_type::text  AS account_type,
               normal_balance,
               parent_id, is_postable, is_contra, requires_subsidiary,
               expense_class::text AS expense_class,
               is_cash, is_active, description
          FROM account
         WHERE ($1::text IS NULL OR code ILIKE '%' || $1 || '%' OR name ILIKE '%' || $1 || '%')
           AND ($2::text IS NULL OR account_type::text = $2)
           AND (NOT $3::boolean OR is_postable)
           AND ($4::boolean OR is_active)
         ORDER BY code
         LIMIT $5 OFFSET $6
        "#,
    )
    .bind(search)
    .bind(filter.account_type.as_deref())
    .bind(postable)
    .bind(inactive)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await
    .context("could not read the chart of accounts")?;

    tx.commit().await?;
    Ok(Page { items, total })
}

/// An account as a picker shows it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AccountOption {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub account_type: String,
    pub expense_class: Option<String>,
    /// The summary account it sits under, e.g. "Cash" over "Cash - Local
    /// Treasury". Lets a picker show where an account belongs.
    pub parent_name: Option<String>,
}

/// Every account an entry can be posted to, for pickers.
///
/// Not paged, unlike [`list_accounts`]. A picker that shows the first twenty
/// and hides the rest is the confusing kind, and a lookup built on one page
/// silently misses everything after it. The postable chart is a few hundred
/// rows, small enough to send once and filter where it is shown.
pub async fn list_account_options(
    pool: &PgPool,
    scope: Scope,
    account_type: Option<&str>,
) -> anyhow::Result<Vec<AccountOption>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, AccountOption>(
        r#"
        SELECT a.id, a.code, a.name,
               a.account_type::text  AS account_type,
               a.expense_class::text AS expense_class,
               p.name                AS parent_name
          FROM account a
          LEFT JOIN account p ON p.id = a.parent_id
         WHERE a.is_postable
           AND a.is_active
           AND ($1::text IS NULL OR a.account_type::text = $1)
         ORDER BY a.code
        "#,
    )
    .bind(account_type)
    .fetch_all(&mut *tx)
    .await
    .context("could not read the chart of accounts")?;

    tx.commit().await?;
    Ok(rows)
}

#[derive(Debug, Deserialize)]
pub struct NewAccount {
    pub code: String,
    pub name: String,
    pub account_type: String,
    pub parent_id: Option<Uuid>,
    pub is_postable: bool,
    pub is_contra: bool,
    pub requires_subsidiary: bool,
    pub expense_class: Option<String>,
    pub is_cash: bool,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn create_account(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    input: NewAccount,
) -> anyhow::Result<Account> {
    let mut tx = scoped(pool, scope).await?;

    let account = sqlx::query_as::<_, Account>(
        r#"
        INSERT INTO account (tenant_id, code, name, account_type, parent_id,
                             is_postable, is_contra, requires_subsidiary,
                             expense_class, is_cash, description)
             VALUES ($1, $2, $3, $4::account_type, $5, $6, $7, $8,
                     $9::expense_class, $10, $11)
          RETURNING id, code, name,
                    account_type::text  AS account_type,
                    normal_balance,
                    parent_id, is_postable, is_contra, requires_subsidiary,
                    expense_class::text AS expense_class,
                    is_cash, is_active, description
        "#,
    )
    .bind(scope.tenant_id)
    .bind(input.code.trim())
    .bind(input.name.trim())
    .bind(&input.account_type)
    .bind(input.parent_id)
    .bind(input.is_postable)
    .bind(input.is_contra)
    .bind(input.requires_subsidiary)
    .bind(input.expense_class.as_deref())
    .bind(input.is_cash)
    .bind(input.description.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .fetch_one(&mut *tx)
    .await?;

    crate::audit(
        &mut tx,
        scope,
        actor_label,
        "create",
        "account",
        Some(account.id),
        None,
        Some(serde_json::to_value(&account)?),
        None,
    )
    .await?;

    tx.commit().await?;
    Ok(account)
}

/// Deactivates an account rather than deleting it.
///
/// An account that has ever been posted to can never be removed — the ledger
/// entries referencing it are permanent, and a report covering an earlier
/// period still has to name it. Deactivation keeps it out of pickers while
/// leaving history readable.
pub async fn deactivate_account(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    id: Uuid,
) -> anyhow::Result<bool> {
    let mut tx = scoped(pool, scope).await?;

    let updated = sqlx::query("UPDATE account SET is_active = false WHERE id = $1 AND is_active")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    if updated > 0 {
        crate::audit(
            &mut tx, scope, actor_label, "deactivate", "account", Some(id),
            None, None, None,
        )
        .await?;
    }

    tx.commit().await?;
    Ok(updated > 0)
}
