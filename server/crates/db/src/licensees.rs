//! Licence profiles: who a key was issued to, and every key issued.
//!
//! Nothing here signs anything. The key is signed in the API, where the
//! authority's private key is held; this only records the outcome.

use anyhow::Context;
use chrono::{DateTime, Months, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{scoped, Refusal, Scope};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewPerson {
    pub full_name: String,
    pub username: String,
    /// `ACCOUNTANT` or `STAFF`.
    pub role: String,
    pub position_title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewLicensee {
    pub municipality_name: String,
    pub target_tenant_id: Uuid,
    pub target_tenant_code: String,
    pub for_this_server: bool,
    pub notes: Option<String>,
    pub people: Vec<NewPerson>,
}

/// A key as issued, for the record.
#[derive(Debug, Clone)]
pub struct IssueRecord {
    pub token: String,
    pub plan: String,
    pub seats: i32,
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub grace_days: i32,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LicenseeSummary {
    pub id: Uuid,
    pub municipality_name: String,
    pub target_tenant_code: String,
    pub for_this_server: bool,
    pub accountant_name: Option<String>,
    pub people_count: i64,
    pub keys_issued: i64,
    pub latest_plan: Option<String>,
    pub latest_seats: Option<i32>,
    pub latest_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LicenseePerson {
    pub full_name: String,
    pub username: String,
    pub role: String,
    pub position_title: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IssuedKey {
    pub id: Uuid,
    pub token: String,
    pub plan: String,
    pub seats: i32,
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub grace_days: i32,
    pub issued_at: DateTime<Utc>,
    pub issued_by_name: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct LicenseeHeader {
    id: Uuid,
    municipality_name: String,
    target_tenant_id: Uuid,
    target_tenant_code: String,
    for_this_server: bool,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    created_by_name: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Renewal {
    pub id: Uuid,
    pub period_start: DateTime<Utc>,
    pub paid_through: DateTime<Utc>,
    pub notes: Option<String>,
    pub renewed_at: DateTime<Utc>,
    pub renewed_by_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LicenseeDetail {
    pub id: Uuid,
    pub municipality_name: String,
    pub target_tenant_id: Uuid,
    pub target_tenant_code: String,
    pub for_this_server: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub created_by_name: Option<String>,
    pub people: Vec<LicenseePerson>,
    /// Newest first. The first is the one currently worth installing.
    pub keys: Vec<IssuedKey>,
    /// `monthly`, `annual`, or `lifetime` — the access of its latest key.
    pub access: Option<String>,
    /// The later of the key's end and the latest renewal: what the
    /// municipality's server is told when it checks in.
    pub paid_through: Option<DateTime<Utc>>,
    /// Newest first.
    pub renewals: Vec<Renewal>,
}

/// The municipality this server serves, by id. `tenant` carries no row-level
/// guard, so this reads outside a scoped transaction.
pub async fn tenant_identity(pool: &PgPool, tenant_id: Uuid) -> anyhow::Result<(String, String)> {
    sqlx::query_as("SELECT code, name FROM tenant WHERE id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .context("could not read this server's municipality")
}

/// The profile already held for a municipality, by its code or its id.
pub async fn find_licensee(
    pool: &PgPool,
    scope: Scope,
    tenant_code: &str,
    tenant_id: Uuid,
) -> anyhow::Result<Option<Uuid>> {
    let mut tx = scoped(pool, scope).await?;

    let id = sqlx::query_scalar(
        "SELECT id FROM licensee WHERE target_tenant_code = $1 OR target_tenant_id = $2 LIMIT 1",
    )
    .bind(tenant_code)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .context("could not check for an existing licence profile")?;

    tx.commit().await?;
    Ok(id)
}

/// Records that a key was generated.
///
/// A signed key is valid the moment it exists, whether or not it is ever
/// saved to a profile, so its making is written to the audit trail on its own.
pub async fn record_generation(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    municipality: &str,
    plan: &str,
    expires_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let mut tx = scoped(pool, scope).await?;

    crate::audit(
        &mut tx, scope, actor_label, "licence.generate", "licensee", None,
        None,
        Some(serde_json::json!({
            "municipality": municipality,
            "plan": plan,
            "expires_at": expires_at,
        })),
        None,
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn list_licensees(pool: &PgPool, scope: Scope) -> anyhow::Result<Vec<LicenseeSummary>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, LicenseeSummary>(
        r#"
        SELECT l.id, l.municipality_name, l.target_tenant_code, l.for_this_server,
               (SELECT p.full_name FROM licensee_person p
                 WHERE p.licensee_id = l.id AND p.role_code = 'ACCOUNTANT'
                 ORDER BY p.sort_order LIMIT 1)                        AS accountant_name,
               (SELECT count(*) FROM licensee_person p
                 WHERE p.licensee_id = l.id)                           AS people_count,
               (SELECT count(*) FROM licence_issue i
                 WHERE i.licensee_id = l.id)                           AS keys_issued,
               k.plan       AS latest_plan,
               k.seats      AS latest_seats,
               -- Paid through: a renewal carries the date on past the key's own.
               greatest(k.expires_at,
                        (SELECT max(r.paid_through) FROM licence_renewal r
                          WHERE r.licensee_id = l.id)) AS latest_expires_at,
               l.created_at
          FROM licensee l
          LEFT JOIN LATERAL (
                SELECT plan, seats, expires_at FROM licence_issue i
                 WHERE i.licensee_id = l.id
                 ORDER BY i.issued_at DESC LIMIT 1
          ) k ON true
         ORDER BY l.created_at DESC
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .context("could not read the licence profiles")?;

    tx.commit().await?;
    Ok(rows)
}

async fn read_licensee(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> anyhow::Result<Option<LicenseeDetail>> {
    let header = sqlx::query_as::<_, LicenseeHeader>(
        r#"
        SELECT l.id, l.municipality_name, l.target_tenant_id, l.target_tenant_code,
               l.for_this_server, l.notes, l.created_at, u.full_name AS created_by_name
          FROM licensee l
          LEFT JOIN app_user u ON u.id = l.created_by
         WHERE l.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(h) = header else {
        return Ok(None);
    };

    let people = sqlx::query_as::<_, LicenseePerson>(
        r#"
        SELECT full_name, username::text AS username, role_code AS role, position_title
          FROM licensee_person
         WHERE licensee_id = $1
         ORDER BY sort_order
        "#,
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;

    let keys = sqlx::query_as::<_, IssuedKey>(
        r#"
        SELECT i.id, i.token, i.plan, i.seats, i.not_before, i.expires_at, i.grace_days,
               i.issued_at, u.full_name AS issued_by_name
          FROM licence_issue i
          LEFT JOIN app_user u ON u.id = i.issued_by
         WHERE i.licensee_id = $1
         ORDER BY i.issued_at DESC
        "#,
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;

    let renewals = sqlx::query_as::<_, Renewal>(
        r#"
        SELECT r.id, r.period_start, r.paid_through, r.notes, r.renewed_at,
               u.full_name AS renewed_by_name
          FROM licence_renewal r
          LEFT JOIN app_user u ON u.id = r.renewed_by
         WHERE r.licensee_id = $1
         ORDER BY r.renewed_at DESC
        "#,
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;

    let access = keys.first().map(|k| k.plan.clone());
    let paid_through = keys
        .first()
        .map(|k| k.expires_at)
        .into_iter()
        .chain(renewals.iter().map(|r| r.paid_through))
        .max();

    Ok(Some(LicenseeDetail {
        id: h.id,
        municipality_name: h.municipality_name,
        target_tenant_id: h.target_tenant_id,
        target_tenant_code: h.target_tenant_code,
        for_this_server: h.for_this_server,
        notes: h.notes,
        created_at: h.created_at,
        created_by_name: h.created_by_name,
        people,
        keys,
        access,
        paid_through,
        renewals,
    }))
}

pub async fn get_licensee(
    pool: &PgPool,
    scope: Scope,
    id: Uuid,
) -> anyhow::Result<Option<LicenseeDetail>> {
    let mut tx = scoped(pool, scope).await?;
    let detail = read_licensee(&mut tx, id).await?;
    tx.commit().await?;
    Ok(detail)
}

async fn insert_issue(
    tx: &mut Transaction<'_, Postgres>,
    scope: Scope,
    licensee_id: Uuid,
    issue: &IssueRecord,
) -> anyhow::Result<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO licence_issue
            (tenant_id, licensee_id, token, plan, seats, not_before, expires_at,
             grace_days, issued_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(licensee_id)
    .bind(&issue.token)
    .bind(&issue.plan)
    .bind(issue.seats)
    .bind(issue.not_before)
    .bind(issue.expires_at)
    .bind(issue.grace_days)
    .bind(scope.user_id)
    .fetch_one(&mut **tx)
    .await?;

    Ok(id)
}

/// Saves a new profile, its people, and its first key, together — a profile
/// with no key, or a key with no profile to find it by, is never left behind.
pub async fn create_licensee(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    input: NewLicensee,
    first_key: IssueRecord,
) -> anyhow::Result<LicenseeDetail> {
    let mut tx = scoped(pool, scope).await?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO licensee
            (tenant_id, municipality_name, target_tenant_id, target_tenant_code,
             for_this_server, notes, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(input.municipality_name.trim())
    .bind(input.target_tenant_id)
    .bind(&input.target_tenant_code)
    .bind(input.for_this_server)
    .bind(input.notes.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(scope.user_id)
    .fetch_one(&mut *tx)
    .await?;

    for (order, person) in input.people.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO licensee_person
                (tenant_id, licensee_id, full_name, username, role_code, position_title, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(scope.tenant_id)
        .bind(id)
        .bind(person.full_name.trim())
        .bind(person.username.trim())
        .bind(&person.role)
        .bind(person.position_title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .bind(i16::try_from(order).unwrap_or(i16::MAX))
        .execute(&mut *tx)
        .await?;
    }

    insert_issue(&mut tx, scope, id, &first_key).await?;

    crate::audit(
        &mut tx, scope, actor_label, "licence.issue", "licensee", Some(id),
        None,
        Some(serde_json::json!({
            "municipality": input.municipality_name,
            "plan": first_key.plan,
            "seats": first_key.seats,
            "expires_at": first_key.expires_at,
            "people": input.people.len(),
        })),
        None,
    )
    .await?;

    let detail = read_licensee(&mut tx, id)
        .await?
        .context("the licence profile could not be read back after saving")?;

    tx.commit().await?;
    Ok(detail)
}

/// Records another key for an existing profile — a renewal, or a change of
/// plan or seats.
pub async fn record_issue(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    licensee_id: Uuid,
    key: IssueRecord,
) -> anyhow::Result<Option<LicenseeDetail>> {
    let mut tx = scoped(pool, scope).await?;

    let exists: bool = sqlx::query_scalar("SELECT exists(SELECT 1 FROM licensee WHERE id = $1)")
        .bind(licensee_id)
        .fetch_one(&mut *tx)
        .await?;

    if !exists {
        return Ok(None);
    }

    insert_issue(&mut tx, scope, licensee_id, &key).await?;

    crate::audit(
        &mut tx, scope, actor_label, "licence.reissue", "licensee", Some(licensee_id),
        None,
        Some(serde_json::json!({
            "plan": key.plan,
            "seats": key.seats,
            "expires_at": key.expires_at,
        })),
        None,
    )
    .await?;

    let detail = read_licensee(&mut tx, licensee_id).await?;
    tx.commit().await?;
    Ok(detail)
}

/// Months one renewal buys, by the access of the profile's key.
///
/// Keys from before access types existed carry a plan such as `standard`;
/// those renew by the year, which is how they were sold.
fn renewal_months(access: &str) -> Option<u32> {
    match access {
        "lifetime" => None,
        "monthly" => Some(1),
        _ => Some(12),
    }
}

/// Records a paid renewal: another month or year of the same subscription.
///
/// No key is issued. The municipality's server collects the new end date the
/// next time it checks in.
///
/// Extends from the current paid-through date if that is still ahead — paying
/// early must not lose the days already paid for — or from now if it has
/// lapsed, so a late payment buys a whole period rather than one half used up.
pub async fn renew_licensee(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    id: Uuid,
    notes: Option<String>,
) -> anyhow::Result<Option<LicenseeDetail>> {
    let mut tx = scoped(pool, scope).await?;

    // Locked, so two renewals recorded at the same moment cannot both extend
    // from the same date and quietly pay for one period twice.
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM licensee WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

    if exists.is_none() {
        return Ok(None);
    }

    let detail = read_licensee(&mut tx, id)
        .await?
        .context("the licence profile could not be read")?;

    let access = detail.access.clone().unwrap_or_default();
    let Some(months) = renewal_months(&access) else {
        return Err(Refusal::new(
            "LIFETIME_NEEDS_NO_RENEWAL",
            "This is a lifetime licence. It never expires, so there is nothing to renew.",
        )
        .into());
    };

    let now = Utc::now();
    let period_start = detail.paid_through.filter(|p| *p > now).unwrap_or(now);
    let paid_through = period_start
        .checked_add_months(Months::new(months))
        .context("the renewal runs past any date the system can record")?;

    let renewal_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO licence_renewal (tenant_id, licensee_id, period_start, paid_through, notes, renewed_by)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(id)
    .bind(period_start)
    .bind(paid_through)
    .bind(notes.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(scope.user_id)
    .fetch_one(&mut *tx)
    .await?;

    crate::audit(
        &mut tx, scope, actor_label, "licence.renew", "licensee", Some(id),
        None,
        Some(serde_json::json!({
            "renewal": renewal_id,
            "access": access,
            "period_start": period_start,
            "paid_through": paid_through,
        })),
        None,
    )
    .await?;

    let detail = read_licensee(&mut tx, id).await?;
    tx.commit().await?;
    Ok(detail)
}

/// A subscription as a checking-in server needs it.
#[derive(Debug, Clone)]
pub struct Standing {
    pub paid_through: DateTime<Utc>,
    pub access: String,
}

/// Where a municipality's subscription stands, found by the municipality the
/// key was issued for. `None` when this licensing server has no profile for it.
pub async fn standing_for_tenant(
    pool: &PgPool,
    scope: Scope,
    target_tenant_id: Uuid,
) -> anyhow::Result<Option<Standing>> {
    let mut tx = scoped(pool, scope).await?;

    let id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM licensee WHERE target_tenant_id = $1")
            .bind(target_tenant_id)
            .fetch_optional(&mut *tx)
            .await?;

    let standing = match id {
        Some(id) => read_licensee(&mut tx, id).await?.and_then(|d| {
            Some(Standing { paid_through: d.paid_through?, access: d.access? })
        }),
        None => None,
    };

    tx.commit().await?;
    Ok(standing)
}
