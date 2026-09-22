//! Database access.
//!
//! Every query in GIAMS runs inside a **tenant-scoped transaction**. The scope
//! is set with `SET LOCAL`, which the database's row level security policies
//! read. A query that forgets its tenant filter therefore returns nothing,
//! rather than another municipality's ledger.
//!
//! This is why [`scoped`] is the only way to obtain a transaction in this
//! crate: there is no unscoped path to accidentally take.

#![forbid(unsafe_code)]

pub mod books;
pub mod dashboard;
pub mod budget;
pub mod disbursement;
pub mod reconciliation;
pub mod reference;
pub mod refusal;
pub mod statements;

use anyhow::Context;
use sqlx::postgres::{PgPoolOptions, PgQueryResult};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub use refusal::Refusal;
pub use sqlx;

/// Connection pool settings tuned for an office server, not a cloud fleet:
/// a handful of workstations, long-lived connections, fail fast when the
/// database is down so the client can fall back to offline drafting.
pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(16)
        .min_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .idle_timeout(std::time::Duration::from_secs(600))
        .connect(database_url)
        .await
        .context("could not connect to the accounting database")
}

/// Who is acting, for row level security and for the audit trail.
#[derive(Debug, Clone, Copy)]
pub struct Scope {
    pub tenant_id: Uuid,
    pub user_id: Option<Uuid>,
}

impl Scope {
    pub fn system(tenant_id: Uuid) -> Self {
        Scope { tenant_id, user_id: None }
    }

    pub fn user(tenant_id: Uuid, user_id: Uuid) -> Self {
        Scope { tenant_id, user_id: Some(user_id) }
    }
}

/// Opens a transaction with the tenant and user in scope.
///
/// `SET LOCAL` is bound to the transaction, so the scope is released on commit
/// or rollback and cannot leak to the next borrower of a pooled connection.
/// That property is what makes it safe to use a shared pool at all.
pub async fn scoped(pool: &PgPool, scope: Scope) -> anyhow::Result<Transaction<'static, Postgres>> {
    let mut tx = pool.begin().await.context("could not start a transaction")?;

    sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
        .bind(scope.tenant_id.to_string())
        .execute(&mut *tx)
        .await
        .context("could not set the tenant scope")?;

    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(scope.user_id.map(|u| u.to_string()).unwrap_or_default())
        .execute(&mut *tx)
        .await
        .context("could not set the user scope")?;

    Ok(tx)
}

/// Applies every migration in `db/migrations` that has not yet run.
///
/// Embedded in the binary, so deploying the server and migrating the database
/// are the same act. An office server should never need a separate migration
/// tool copied onto it.
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("../../../db/migrations")
        .run(pool)
        .await
        .context("could not apply database migrations")?;
    Ok(())
}

/// Confirms the database is reachable and answering.
pub async fn ping(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

/// Verifies a tenant's audit trail has not been tampered with.
///
/// Returns the problems found; an empty vector means the trail is intact.
/// Intended to be run on a schedule and before any COA submission.
pub async fn verify_audit_chain(
    pool: &PgPool,
    tenant_id: Uuid,
) -> anyhow::Result<Vec<AuditChainProblem>> {
    let rows = sqlx::query_as::<_, AuditChainProblem>(
        "SELECT audit_id, seq, at, problem FROM verify_audit_chain($1)",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .context("could not verify the audit trail")?;

    Ok(rows)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct AuditChainProblem {
    pub audit_id: i64,
    pub seq: i64,
    pub at: chrono::DateTime<chrono::Utc>,
    pub problem: String,
}

/// Records an entry in the tamper-evident audit trail.
///
/// Must be called inside the same transaction as the change it describes, so
/// that a change can never exist without its audit record.
pub async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    scope: Scope,
    actor_label: &str,
    action: &str,
    entity: &str,
    entity_id: Option<Uuid>,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
    reference: Option<&str>,
) -> anyhow::Result<PgQueryResult> {
    sqlx::query(
        r#"
        INSERT INTO audit_log
            (tenant_id, actor_id, actor_label, action, entity, entity_id, before, after, reference)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(scope.tenant_id)
    .bind(scope.user_id)
    .bind(actor_label)
    .bind(action)
    .bind(entity)
    .bind(entity_id)
    .bind(before)
    .bind(after)
    .bind(reference)
    .execute(&mut **tx)
    .await
    .context("could not write the audit record")
}
