//! The office's own view of where things stand.
//!
//! Deliberately not a wall of charts. An accounting office needs three things
//! from a summary screen: how much cash there is, what moved this period, and
//! what is sitting on somebody's desk waiting to be dealt with. The last of
//! those is the reason this screen exists — a voucher certified in March and
//! never approved is invisible in every other view in the system, because
//! every other view is organised by document rather than by what is stuck.
//!
//! Figures are computed from posted entries only, by the same rules the
//! statements use. A dashboard that flattered the ledger would be worse than
//! no dashboard.

use anyhow::Context;
use giams_domain::Money;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{scoped, Scope};

#[derive(Debug, Serialize)]
pub struct Dashboard {
    pub period_label: String,
    pub as_of: chrono::NaiveDate,
    pub figures: Figures,
    pub attention: Vec<Attention>,
    pub budget: Vec<BudgetUse>,
}

#[derive(Debug, Serialize)]
pub struct Figures {
    /// Every cash and cash-equivalent account, as at the end of the period.
    pub cash: Money,
    /// Cash received in the period — debits to cash, whichever book they came
    /// through.
    pub received: Money,
    /// Cash paid out in the period.
    pub paid: Money,
    /// Income less expenses for the period, before closing.
    pub surplus: Money,
}

#[derive(Debug, sqlx::FromRow)]
struct FiguresRow {
    cash: Decimal,
    received: Decimal,
    paid: Decimal,
    surplus: Decimal,
}

/// Something waiting on a person, with the count and where to go.
#[derive(Debug, Serialize)]
pub struct Attention {
    /// Stable key, so the client can route without matching on prose.
    pub key: String,
    pub label: String,
    pub count: i64,
    /// The screen that deals with it.
    pub route: String,
    /// Whether this is merely pending or genuinely overdue.
    pub urgent: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct AttentionRow {
    jev_drafts: i64,
    dv_awaiting_certification: i64,
    dv_awaiting_approval: i64,
    dv_approved_unpaid: i64,
    obligations_uncertified: i64,
    bank_lines_unmatched: i64,
    periods_overdue: i64,
}

#[derive(Debug, Serialize)]
pub struct BudgetUse {
    pub expense_class: String,
    pub budget: Money,
    pub obligated: Money,
    pub disbursed: Money,
    /// Obligations as a share of the final budget, to two places.
    pub utilisation_rate: String,
}

#[derive(Debug, sqlx::FromRow)]
struct BudgetUseRow {
    expense_class: String,
    budget: Decimal,
    obligated: Decimal,
    disbursed: Decimal,
    utilisation_rate: Decimal,
}

pub async fn dashboard(
    pool: &PgPool,
    scope: Scope,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    fund_id: Option<Uuid>,
) -> anyhow::Result<Dashboard> {
    let mut tx = scoped(pool, scope).await?;

    // Cash is read as at the end of the period from the same trial balance the
    // statements use, rather than by summing movements — a balance derived two
    // different ways is a balance that will eventually disagree with itself.
    let figures = sqlx::query_as::<_, FiguresRow>(
        r#"
        WITH cash_accounts AS (
            SELECT id FROM account
             WHERE tenant_id = $1 AND account_type = 'asset'
               AND code LIKE '1-01-%'
        ),
        movement AS (
            SELECT coalesce(sum(l.debit),  0)::numeric(18,2) AS received,
                   coalesce(sum(l.credit), 0)::numeric(18,2) AS paid
              FROM journal_entry_line l
              JOIN journal_entry je ON je.id = l.entry_id
             WHERE je.tenant_id  = $1
               AND je.status     = 'posted'
               AND je.entry_date BETWEEN $2 AND $3
               AND ($4::uuid IS NULL OR je.fund_id = $4)
               AND l.account_id IN (SELECT id FROM cash_accounts)
        ),
        result AS (
            -- Income adds credits and expenses subtract debits, so both sides
            -- reduce to the same expression. Income less expenses, in one sum.
            SELECT coalesce(sum(l.credit - l.debit), 0)::numeric(18,2) AS surplus
              FROM journal_entry_line l
              JOIN journal_entry je ON je.id = l.entry_id
              JOIN account a        ON a.id  = l.account_id
             WHERE je.tenant_id  = $1
               AND je.status     = 'posted'
               AND je.entry_date BETWEEN $2 AND $3
               AND ($4::uuid IS NULL OR je.fund_id = $4)
               AND a.account_type IN ('income', 'expense')
        ),
        cash AS (
            SELECT coalesce(sum(tb.balance), 0)::numeric(18,2) AS cash
              FROM trial_balance_as_of($1, $3, $4) tb
             WHERE tb.account_code LIKE '1-01-%'
        )
        SELECT cash.cash, movement.received, movement.paid, result.surplus
          FROM cash, movement, result
        "#,
    )
    .bind(scope.tenant_id)
    .bind(from)
    .bind(to)
    .bind(fund_id)
    .fetch_one(&mut *tx)
    .await
    .context("could not read the period's figures")?;

    // Counted as at today, not as at the period — something stuck in January
    // is still stuck when you are looking at August, and hiding it behind the
    // period picker is how it stays stuck.
    let counts = sqlx::query_as::<_, AttentionRow>(
        r#"
        SELECT
          (SELECT count(*) FROM journal_entry
            WHERE tenant_id = $1 AND status = 'draft')                    AS jev_drafts,
          (SELECT count(*) FROM disbursement_voucher
            WHERE tenant_id = $1 AND status = 'submitted')                AS dv_awaiting_certification,
          (SELECT count(*) FROM disbursement_voucher
            WHERE tenant_id = $1 AND status = 'certified')                AS dv_awaiting_approval,
          (SELECT count(*) FROM disbursement_voucher dv
            WHERE dv.tenant_id = $1 AND dv.status = 'approved'
              AND NOT EXISTS (SELECT 1 FROM disbursement d
                               WHERE d.dv_id = dv.id AND d.status <> 'cancelled'))
                                                                          AS dv_approved_unpaid,
          (SELECT count(*) FROM obligation
            WHERE tenant_id = $1 AND status IN ('draft', 'submitted'))    AS obligations_uncertified,
          (SELECT count(*) FROM bank_statement_line l
            WHERE l.tenant_id = $1 AND l.matched_kind IS NULL)            AS bank_lines_unmatched,
          -- A month that ended more than a month ago and is still open. The
          -- deadline COA cares about, expressed the way the office thinks of it.
          (SELECT count(*) FROM posting_period
            WHERE tenant_id = $1 AND status = 'open'
              AND ends_on < (current_date - INTERVAL '1 month'))          AS periods_overdue
        "#,
    )
    .bind(scope.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .context("could not count what is waiting")?;

    // Reuses the budget statement rather than recomputing it, so the dashboard
    // and the SCBAA can never tell the office two different numbers.
    let budget = sqlx::query_as::<_, BudgetUseRow>(
        r#"
        SELECT s.expense_class,
               sum(s.final_budget)::numeric(18,2) AS budget,
               sum(s.obligated)::numeric(18,2)    AS obligated,
               sum(s.disbursed)::numeric(18,2)    AS disbursed,
               CASE WHEN sum(s.final_budget) > 0
                    THEN round(sum(s.obligated) * 100 / sum(s.final_budget), 2)
                    ELSE 0 END::numeric(5,2)      AS utilisation_rate
          FROM statement_budget_vs_actual($1, $2::smallint, $3) s
         GROUP BY s.expense_class
         ORDER BY s.expense_class
        "#,
    )
    .bind(scope.tenant_id)
    .bind(chrono::Datelike::year(&to) as i16)
    .bind(fund_id)
    .fetch_all(&mut *tx)
    .await
    .context("could not read how much of the budget has been used")?;

    tx.commit().await?;

    Ok(Dashboard {
        period_label: format!("{from} to {to}"),
        as_of: to,
        figures: Figures {
            cash: Money::new(figures.cash),
            received: Money::new(figures.received),
            paid: Money::new(figures.paid),
            surplus: Money::new(figures.surplus),
        },
        attention: attention_items(&counts),
        budget: budget
            .into_iter()
            .map(|r| BudgetUse {
                expense_class: r.expense_class,
                budget: Money::new(r.budget),
                obligated: Money::new(r.obligated),
                disbursed: Money::new(r.disbursed),
                utilisation_rate: r.utilisation_rate.to_string(),
            })
            .collect(),
    })
}

/// Turns the counts into a list, dropping everything that is at zero.
///
/// A list of "0 waiting" rows trains people to stop reading it, which defeats
/// the purpose. An empty list is a good day, and says so.
fn attention_items(c: &AttentionRow) -> Vec<Attention> {
    let candidates = [
        Attention {
            key: "periods_overdue".into(),
            label: "Months still open past their closing date".into(),
            count: c.periods_overdue,
            route: "/system".into(),
            urgent: true,
        },
        Attention {
            key: "dv_awaiting_certification".into(),
            label: "Vouchers waiting to be certified".into(),
            count: c.dv_awaiting_certification,
            route: "/vouchers".into(),
            urgent: false,
        },
        Attention {
            key: "dv_awaiting_approval".into(),
            label: "Vouchers waiting for approval".into(),
            count: c.dv_awaiting_approval,
            route: "/vouchers".into(),
            urgent: false,
        },
        Attention {
            key: "dv_approved_unpaid".into(),
            label: "Vouchers approved but not yet paid".into(),
            count: c.dv_approved_unpaid,
            route: "/vouchers".into(),
            urgent: false,
        },
        Attention {
            key: "obligations_uncertified".into(),
            label: "Obligation requests not yet certified".into(),
            count: c.obligations_uncertified,
            route: "/obligations".into(),
            urgent: false,
        },
        Attention {
            key: "jev_drafts".into(),
            label: "Journal entries still in draft".into(),
            count: c.jev_drafts,
            route: "/books".into(),
            urgent: false,
        },
        Attention {
            key: "bank_lines_unmatched".into(),
            label: "Bank statement lines not yet matched".into(),
            count: c.bank_lines_unmatched,
            route: "/reconcile".into(),
            urgent: false,
        },
    ];

    candidates.into_iter().filter(|a| a.count > 0).collect()
}
