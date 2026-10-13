//! User accounts on this server, for the admin console.
//!
//! Passwords arrive already hashed. Hashing is the API crate's job, and a
//! function here that took a plain password would be one more place one could
//! end up in a log.

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{scoped, Refusal, Scope};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Account {
    pub id: Uuid,
    pub username: String,
    pub full_name: String,
    pub position_title: Option<String>,
    pub role_codes: Vec<String>,
    pub role_names: Vec<String>,
    pub is_active: bool,
    pub must_change_password: bool,
    /// Locked out after too many failed sign-ins, and not yet released.
    pub locked: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Shared by the list and the single read, so an account reads the same
/// whether it was just created or listed.
const SELECT_ACCOUNT: &str = r#"
    SELECT u.id, u.username::text AS username, u.full_name, u.position_title,
           coalesce(array_agg(r.code ORDER BY r.code) FILTER (WHERE r.code IS NOT NULL), '{}')
               AS role_codes,
           coalesce(array_agg(r.name ORDER BY r.code) FILTER (WHERE r.name IS NOT NULL), '{}')
               AS role_names,
           u.is_active, u.must_change_password,
           (u.locked_until IS NOT NULL AND u.locked_until > now()) AS locked,
           u.last_login_at, u.created_at
      FROM app_user u
      LEFT JOIN user_role ur ON ur.user_id = u.id
      LEFT JOIN role      r  ON r.id = ur.role_id
"#;

pub async fn list_accounts(pool: &PgPool, scope: Scope) -> anyhow::Result<Vec<Account>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, Account>(&format!(
        "{SELECT_ACCOUNT} GROUP BY u.id ORDER BY u.is_active DESC, u.full_name"
    ))
    .fetch_all(&mut *tx)
    .await
    .context("could not read the user accounts")?;

    tx.commit().await?;
    Ok(rows)
}

async fn read_account(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> anyhow::Result<Account> {
    sqlx::query_as::<_, Account>(&format!("{SELECT_ACCOUNT} WHERE u.id = $1 GROUP BY u.id"))
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .context("the account could not be read back after saving")
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RoleOption {
    pub code: String,
    pub name: String,
    pub description: String,
}

pub async fn list_roles(pool: &PgPool, scope: Scope) -> anyhow::Result<Vec<RoleOption>> {
    let mut tx = scoped(pool, scope).await?;

    let rows = sqlx::query_as::<_, RoleOption>("SELECT code, name, description FROM role ORDER BY name")
        .fetch_all(&mut *tx)
        .await
        .context("could not read the roles")?;

    tx.commit().await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewAccount {
    pub username: String,
    pub full_name: String,
    pub position_title: Option<String>,
    /// A role code, such as `ACCOUNTANT`.
    pub role: String,
}

/// Inserts one account and its role. Callers hold the transaction, so a batch
/// either creates everyone or no one.
async fn insert_account(
    tx: &mut Transaction<'_, Postgres>,
    scope: Scope,
    input: &NewAccount,
    password_hash: &str,
) -> anyhow::Result<Uuid> {
    let role_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM role WHERE code = $1")
        .bind(&input.role)
        .fetch_optional(&mut **tx)
        .await?;

    let role_id = role_id.ok_or_else(|| {
        Refusal::new(
            "ROLE_UNKNOWN",
            format!("There is no role called {} on this server.", input.role),
        )
    })?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO app_user (tenant_id, username, full_name, position_title,
                              password_hash, must_change_password)
             VALUES ($1, $2, $3, $4, $5, true)
          RETURNING id
        "#,
    )
    .bind(scope.tenant_id)
    .bind(input.username.trim())
    .bind(input.full_name.trim())
    .bind(input.position_title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(password_hash)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO user_role (tenant_id, user_id, role_id, granted_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(scope.tenant_id)
    .bind(id)
    .bind(role_id)
    .bind(scope.user_id)
    .execute(&mut **tx)
    .await?;

    Ok(id)
}

pub async fn create_account(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    input: NewAccount,
    password_hash: &str,
) -> anyhow::Result<Account> {
    let mut tx = scoped(pool, scope).await?;

    let id = insert_account(&mut tx, scope, &input, password_hash).await?;

    crate::audit(
        &mut tx, scope, actor_label, "user.create", "app_user", Some(id),
        None,
        Some(serde_json::json!({ "username": input.username, "role": input.role })),
        None,
    )
    .await?;

    let account = read_account(&mut tx, id).await?;
    tx.commit().await?;
    Ok(account)
}

/// What happened to one person when a licence was installed.
#[derive(Debug, Serialize)]
pub struct Provisioned {
    pub username: String,
    pub full_name: String,
    pub role: String,
    /// `created`, or `existing` when the account was already here — a renewal
    /// carries the same people, and must not reset anyone's password.
    pub status: &'static str,
}

/// Creates the accounts a licence carries, skipping any that already exist.
///
/// One transaction: a key that names five people either leaves five accounts
/// or none, never a half-staffed office.
pub async fn provision_accounts(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    people: &[(NewAccount, String)],
) -> anyhow::Result<Vec<Provisioned>> {
    let mut tx = scoped(pool, scope).await?;
    let mut outcome = Vec::with_capacity(people.len());

    for (person, password_hash) in people {
        let exists: bool = sqlx::query_scalar(
            "SELECT exists(SELECT 1 FROM app_user WHERE username = $1)",
        )
        .bind(person.username.trim())
        .fetch_one(&mut *tx)
        .await?;

        let status = if exists {
            "existing"
        } else {
            let id = insert_account(&mut tx, scope, person, password_hash).await?;
            crate::audit(
                &mut tx, scope, actor_label, "user.provision", "app_user", Some(id),
                None,
                Some(serde_json::json!({ "username": person.username, "role": person.role })),
                None,
            )
            .await?;
            "created"
        };

        outcome.push(Provisioned {
            username: person.username.clone(),
            full_name: person.full_name.clone(),
            role: person.role.clone(),
            status,
        });
    }

    tx.commit().await?;
    Ok(outcome)
}

/// Signs every session of an account out.
async fn revoke_sessions(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    reason: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE user_session SET revoked_at = now(), revoked_reason = $2
          WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .bind(reason)
    .execute(&mut **tx)
    .await
    .context("could not sign the account out")?;
    Ok(())
}

/// Whether anyone other than `except` can still manage accounts.
///
/// Guards against the one mistake an admin console cannot recover from on its
/// own: switching off the last person able to switch anyone back on.
async fn another_manager_remains(
    tx: &mut Transaction<'_, Postgres>,
    except: Uuid,
) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
          FROM app_user u
         WHERE u.is_active
           AND u.id <> $1
           AND EXISTS (SELECT 1
                         FROM user_role ur
                         JOIN role_permission rp ON rp.role_id = ur.role_id
                        WHERE ur.user_id = u.id
                          AND rp.permission_code = 'user.manage')
        "#,
    )
    .bind(except)
    .fetch_one(&mut **tx)
    .await?;

    Ok(count > 0)
}

pub async fn set_account_active(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    id: Uuid,
    active: bool,
) -> anyhow::Result<Option<Account>> {
    let mut tx = scoped(pool, scope).await?;

    if !active && !another_manager_remains(&mut tx, id).await? {
        let manages: bool = sqlx::query_scalar(
            r#"
            SELECT exists(SELECT 1
                            FROM user_role ur
                            JOIN role_permission rp ON rp.role_id = ur.role_id
                           WHERE ur.user_id = $1 AND rp.permission_code = 'user.manage')
            "#,
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;

        if manages {
            return Err(Refusal::with_hint(
                "LAST_ADMINISTRATOR",
                "This is the only active account that can manage users, so it cannot be switched off.",
                "Give another person the Administrator role first.",
            )
            .into());
        }
    }

    // Switching an account back on also clears a lockout; that is almost always
    // why someone is being switched back on.
    let found: Option<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE app_user
           SET is_active = $2,
               failed_attempts = CASE WHEN $2 THEN 0 ELSE failed_attempts END,
               locked_until    = CASE WHEN $2 THEN NULL ELSE locked_until END
         WHERE id = $1
     RETURNING id
        "#,
    )
    .bind(id)
    .bind(active)
    .fetch_optional(&mut *tx)
    .await?;

    if found.is_none() {
        return Ok(None);
    }

    if !active {
        revoke_sessions(&mut tx, id, "revoked_by_admin").await?;
    }

    crate::audit(
        &mut tx, scope, actor_label,
        if active { "user.activate" } else { "user.deactivate" },
        "app_user", Some(id), None, None, None,
    )
    .await?;

    let account = read_account(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Some(account))
}

/// Replaces a password with a one-time one that must be changed at sign-in,
/// and signs the account out everywhere.
pub async fn reset_account_password(
    pool: &PgPool,
    scope: Scope,
    actor_label: &str,
    id: Uuid,
    password_hash: &str,
) -> anyhow::Result<Option<Account>> {
    let mut tx = scoped(pool, scope).await?;

    let found: Option<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE app_user
           SET password_hash = $2, must_change_password = true,
               password_changed_at = now(), failed_attempts = 0, locked_until = NULL
         WHERE id = $1
     RETURNING id
        "#,
    )
    .bind(id)
    .bind(password_hash)
    .fetch_optional(&mut *tx)
    .await?;

    if found.is_none() {
        return Ok(None);
    }

    revoke_sessions(&mut tx, id, "password_changed").await?;

    crate::audit(
        &mut tx, scope, actor_label, "password.reset", "app_user", Some(id),
        None, None, None,
    )
    .await?;

    let account = read_account(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Some(account))
}
