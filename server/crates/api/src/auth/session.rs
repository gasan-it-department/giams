//! Signing in, refreshing, and signing out.
//!
//! The session model is a **rotating refresh token family**. Each refresh
//! issues a new token and retires the one used. Presenting a retired token
//! means it was captured — the legitimate client would have moved on — so the
//! response is to revoke everything and force a fresh sign-in.

use chrono::{Duration, Utc};
use giams_db::{scoped, Scope};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::password::{hash_password, verify_password, waste_time_like_a_real_verification};
use super::token::{hash_refresh_token, new_refresh_token, TokenKeys};
use crate::AppError;

/// How many consecutive failures before the account is held.
const LOCKOUT_THRESHOLD: i16 = 5;
/// How long the hold lasts. Long enough to make guessing impractical, short
/// enough that a clerk who fat-fingered their password five times is not
/// waiting on an administrator to get back to their desk.
const LOCKOUT_MINUTES: i64 = 15;

pub struct SignedIn {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: chrono::DateTime<Utc>,
    pub user_id: Uuid,
    pub username: String,
    pub full_name: String,
    pub must_change_password: bool,
    pub permissions: Vec<String>,
}

struct UserRow {
    id: Uuid,
    username: String,
    full_name: String,
    password_hash: String,
    is_active: bool,
    failed_attempts: i16,
    locked_until: Option<chrono::DateTime<Utc>>,
    must_change_password: bool,
}

/// The message shown for every kind of sign-in failure.
///
/// Identical whether the username does not exist, the password is wrong, or
/// the account is deactivated. Distinguishing them would let anyone with the
/// login page enumerate the staff of the Accounting Office.
fn sign_in_rejected() -> AppError {
    AppError::Rule {
        code: "SIGN_IN_FAILED",
        message: "That username and password do not match.".into(),
        hint: Some("Check the caps lock key, and try again.".into()),
    }
}

pub async fn sign_in(
    pool: &PgPool,
    keys: &TokenKeys,
    tenant_id: Uuid,
    username: &str,
    password: &str,
    workstation: Option<&str>,
) -> Result<SignedIn, AppError> {
    let mut tx = scoped(pool, Scope::system(tenant_id))
        .await
        .map_err(AppError::Unexpected)?;

    let user = sqlx::query_as::<_, (Uuid, String, String, String, bool, i16, Option<chrono::DateTime<Utc>>, bool)>(
        r#"
        SELECT id, username, full_name, password_hash, is_active,
               failed_attempts, locked_until, must_change_password
          FROM app_user
         WHERE tenant_id = $1 AND username = $2
        "#,
    )
    .bind(tenant_id)
    .bind(username)
    .fetch_optional(&mut *tx)
    .await?
    .map(|r| UserRow {
        id: r.0, username: r.1, full_name: r.2, password_hash: r.3,
        is_active: r.4, failed_attempts: r.5, locked_until: r.6,
        must_change_password: r.7,
    });

    let Some(user) = user else {
        // Spend the same time we would have spent verifying, so a missing
        // username is not detectable by how quickly the answer comes back.
        waste_time_like_a_real_verification();
        return Err(sign_in_rejected());
    };

    if let Some(locked_until) = user.locked_until {
        if locked_until > Utc::now() {
            let minutes = (locked_until - Utc::now()).num_minutes().max(1);
            return Err(AppError::Rule {
                code: "ACCOUNT_LOCKED",
                message: format!(
                    "This account is temporarily locked after too many sign-in attempts. \
                     Try again in {minutes} minute{}.",
                    if minutes == 1 { "" } else { "s" }
                ),
                hint: Some("If you have forgotten your password, ask an administrator to reset it.".into()),
            });
        }
    }

    if !verify_password(password, &user.password_hash) || !user.is_active {
        let attempts = user.failed_attempts + 1;
        let lock_until = (attempts >= LOCKOUT_THRESHOLD)
            .then(|| Utc::now() + Duration::minutes(LOCKOUT_MINUTES));

        sqlx::query(
            "UPDATE app_user SET failed_attempts = $2, locked_until = $3 WHERE id = $1",
        )
        .bind(user.id)
        .bind(attempts)
        .bind(lock_until)
        .execute(&mut *tx)
        .await?;

        giams_db::audit(
            &mut tx, Scope::system(tenant_id), &user.username,
            "sign_in_failed", "app_user", Some(user.id), None, None, None,
        ).await.map_err(AppError::Unexpected)?;

        tx.commit().await?;
        return Err(sign_in_rejected());
    }

    // Success: clear the counter and record the sign-in.
    sqlx::query(
        "UPDATE app_user
            SET failed_attempts = 0, locked_until = NULL, last_login_at = now()
          WHERE id = $1",
    )
    .bind(user.id)
    .execute(&mut *tx)
    .await?;

    let permissions = load_permissions(&mut tx, user.id).await?;
    let refresh = new_refresh_token();

    let session_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO user_session (tenant_id, user_id, token_hash, workstation, expires_at)
             VALUES ($1, $2, $3, $4, $5)
          RETURNING id
        "#,
    )
    .bind(tenant_id)
    .bind(user.id)
    .bind(&refresh.hash)
    .bind(workstation)
    .bind(refresh.expires_at)
    .fetch_one(&mut *tx)
    .await?;

    giams_db::audit(
        &mut tx, Scope::user(tenant_id, user.id), &user.username,
        "sign_in", "user_session", Some(session_id), None, None, None,
    ).await.map_err(AppError::Unexpected)?;

    let (access_token, expires_at) = keys
        .issue_access_token(user.id, tenant_id, session_id, &user.username, permissions.clone())
        .map_err(AppError::Unexpected)?;

    tx.commit().await?;

    Ok(SignedIn {
        access_token,
        refresh_token: refresh.value,
        expires_at,
        user_id: user.id,
        username: user.username,
        full_name: user.full_name,
        must_change_password: user.must_change_password,
        permissions,
    })
}

/// Exchanges a refresh token for a new pair, rotating it.
pub async fn refresh(
    pool: &PgPool,
    keys: &TokenKeys,
    tenant_id: Uuid,
    presented: &str,
) -> Result<SignedIn, AppError> {
    let mut tx = scoped(pool, Scope::system(tenant_id))
        .await
        .map_err(AppError::Unexpected)?;

    let presented_hash = hash_refresh_token(presented);

    let row = sqlx::query_as::<_, (Uuid, Uuid, Option<chrono::DateTime<Utc>>, chrono::DateTime<Utc>)>(
        r#"
        SELECT id, user_id, revoked_at, expires_at
          FROM user_session
         WHERE tenant_id = $1 AND token_hash = $2
        "#,
    )
    .bind(tenant_id)
    .bind(&presented_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((session_id, user_id, revoked_at, expires_at)) = row else {
        return Err(AppError::Unauthenticated);
    };

    // ── Reuse detection ─────────────────────────────────────────────────────
    // This token was already spent. The legitimate client would be holding its
    // replacement, so something else has a copy. Revoking only this session
    // would leave the thief's rotated token working, so every session for the
    // user goes.
    if revoked_at.is_some() {
        sqlx::query(
            "UPDATE user_session
                SET revoked_at = now(), revoked_reason = 'reuse_detected'
              WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(tenant_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        giams_db::audit(
            &mut tx, Scope::system(tenant_id), "system",
            "refresh_token_reused", "user_session", Some(session_id), None, None, None,
        ).await.map_err(AppError::Unexpected)?;

        tx.commit().await?;

        tracing::warn!(%user_id, %session_id, "retired refresh token replayed; all sessions revoked");
        return Err(AppError::Unauthenticated);
    }

    if expires_at <= Utc::now() {
        return Err(AppError::Unauthenticated);
    }

    let (username, full_name, is_active, must_change_password) =
        sqlx::query_as::<_, (String, String, bool, bool)>(
            "SELECT username, full_name, is_active, must_change_password FROM app_user WHERE id = $1",
        )
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

    if !is_active {
        return Err(AppError::Unauthenticated);
    }

    let permissions = load_permissions(&mut tx, user_id).await?;
    let next = new_refresh_token();

    // Retire the presented token and issue its successor, linked so the family
    // is traceable.
    sqlx::query(
        "UPDATE user_session
            SET revoked_at = now(), revoked_reason = 'rotated', last_used_at = now()
          WHERE id = $1",
    )
    .bind(session_id)
    .execute(&mut *tx)
    .await?;

    let new_session_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO user_session (tenant_id, user_id, token_hash, previous_id, expires_at)
             VALUES ($1, $2, $3, $4, $5)
          RETURNING id
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(&next.hash)
    .bind(session_id)
    .bind(next.expires_at)
    .fetch_one(&mut *tx)
    .await?;

    let (access_token, access_expires) = keys
        .issue_access_token(user_id, tenant_id, new_session_id, &username, permissions.clone())
        .map_err(AppError::Unexpected)?;

    tx.commit().await?;

    Ok(SignedIn {
        access_token,
        refresh_token: next.value,
        expires_at: access_expires,
        user_id,
        username,
        full_name,
        must_change_password,
        permissions,
    })
}

pub async fn sign_out(pool: &PgPool, tenant_id: Uuid, presented: &str) -> Result<(), AppError> {
    let mut tx = scoped(pool, Scope::system(tenant_id))
        .await
        .map_err(AppError::Unexpected)?;

    sqlx::query(
        "UPDATE user_session
            SET revoked_at = now(), revoked_reason = 'logout'
          WHERE tenant_id = $1 AND token_hash = $2 AND revoked_at IS NULL",
    )
    .bind(tenant_id)
    .bind(hash_refresh_token(presented))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

async fn load_permissions(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Vec<String>, AppError> {
    let perms = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT rp.permission_code
          FROM user_role ur
          JOIN role_permission rp ON rp.role_id = ur.role_id
         WHERE ur.user_id = $1
         ORDER BY rp.permission_code
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;

    Ok(perms)
}

/// Sets a new password and revokes every existing session.
///
/// Revoking on password change is the point of changing it: if the old one was
/// known to someone else, any session they opened with it must die too.
pub async fn change_password(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    current: &str,
    new: &str,
) -> Result<(), AppError> {
    if new.chars().count() < 12 {
        return Err(AppError::validation(
            "new_password",
            "Use at least 12 characters. A short phrase you can remember is stronger than a short complicated word.",
        ));
    }

    let mut tx = scoped(pool, Scope::user(tenant_id, user_id))
        .await
        .map_err(AppError::Unexpected)?;

    let (stored, username) =
        sqlx::query_as::<_, (String, String)>("SELECT password_hash, username FROM app_user WHERE id = $1")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;

    if !verify_password(current, &stored) {
        return Err(AppError::validation(
            "current_password",
            "That is not your current password.",
        ));
    }

    let hashed = hash_password(new).map_err(AppError::Unexpected)?;

    sqlx::query(
        "UPDATE app_user
            SET password_hash = $2, password_changed_at = now(), must_change_password = false
          WHERE id = $1",
    )
    .bind(user_id)
    .bind(&hashed)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE user_session
            SET revoked_at = now(), revoked_reason = 'password_changed'
          WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    giams_db::audit(
        &mut tx, Scope::user(tenant_id, user_id), &username,
        "password_changed", "app_user", Some(user_id), None, None, None,
    ).await.map_err(AppError::Unexpected)?;

    tx.commit().await?;
    Ok(())
}
