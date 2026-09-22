//! Sign-in, refresh, sign-out, and the current user.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::{session, CurrentUser};
use crate::{ApiResult, AppError, AppState};
use crate::extract::Json as JsonBody;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/refresh", post(refresh))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/auth/password", post(change_password))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
    /// Which workstation this is, recorded on the session so an administrator
    /// can see where a person is signed in.
    workstation: Option<String>,
}

#[derive(Serialize)]
pub struct SessionResponse {
    access_token: String,
    refresh_token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    user: UserSummary,
}

#[derive(Serialize)]
pub struct UserSummary {
    id: uuid::Uuid,
    username: String,
    full_name: String,
    must_change_password: bool,
    permissions: Vec<String>,
}

impl From<session::SignedIn> for SessionResponse {
    fn from(s: session::SignedIn) -> Self {
        SessionResponse {
            access_token: s.access_token,
            refresh_token: s.refresh_token,
            expires_at: s.expires_at,
            user: UserSummary {
                id: s.user_id,
                username: s.username,
                full_name: s.full_name,
                must_change_password: s.must_change_password,
                permissions: s.permissions,
            },
        }
    }
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(caller): ConnectInfo<SocketAddr>,
    JsonBody(body): JsonBody<LoginRequest>,
) -> ApiResult<Json<SessionResponse>> {
    if body.username.trim().is_empty() {
        return Err(AppError::validation("username", "Enter your username."));
    }
    if body.password.is_empty() {
        return Err(AppError::validation("password", "Enter your password."));
    }

    // The account itself locks after repeated failures, but an attacker trying
    // one password against every username never trips that — each account sees
    // a single failure. Counting the source is the shape that attack has.
    let address = caller.ip();

    if !state.login_limiter.permitted(address) {
        tracing::warn!(%address, "sign-in attempts from this address have been rate limited");
        return Err(crate::hardening::too_many_attempts());
    }

    let outcome = session::sign_in(
        &state.pool,
        &state.keys,
        state.node.tenant_id,
        body.username.trim(),
        &body.password,
        body.workstation.as_deref(),
    )
    .await;

    match outcome {
        Ok(_) => state.login_limiter.succeeded(address),
        Err(_) => state.login_limiter.failed(address),
    }

    Ok(Json(outcome?.into()))
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    refresh_token: String,
}

async fn refresh(
    State(state): State<AppState>,
    JsonBody(body): JsonBody<RefreshRequest>,
) -> ApiResult<Json<SessionResponse>> {
    let signed_in = session::refresh(
        &state.pool,
        &state.keys,
        state.node.tenant_id,
        &body.refresh_token,
    )
    .await?;

    Ok(Json(signed_in.into()))
}

async fn logout(
    State(state): State<AppState>,
    JsonBody(body): JsonBody<RefreshRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    session::sign_out(&state.pool, state.node.tenant_id, &body.refresh_token).await?;
    Ok(Json(serde_json::json!({ "signed_out": true })))
}

/// Who the caller is. Naming `CurrentUser` here is what makes this route
/// require authentication.
async fn me(user: CurrentUser) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "id": user.user_id,
        "username": user.username,
        "permissions": user.permissions(),
    })))
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(body): JsonBody<ChangePasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    session::change_password(
        &state.pool,
        user.tenant_id,
        user.user_id,
        &body.current_password,
        &body.new_password,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "changed": true,
        "message": "Your password has been changed. Sign in again with the new one."
    })))
}
