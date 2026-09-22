//! Uploading to the cloud: status, and asking for it now.
//!
//! The user asked for uploading to be either automatic or on demand, and both
//! live here. What neither does is report an upload problem as though it were
//! a problem with the books — a voucher recorded on a node with no internet is
//! recorded, full stop, and the screen says so.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::uploader::SyncStatus;
use crate::{ApiResult, AppState, CurrentUser};
use crate::extract::Json as JsonBody;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/sync/status", get(status))
        .route("/api/sync/upload", post(upload_now))
        .route("/api/sync/automatic", post(set_automatic))
}

async fn status(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<SyncStatus>> {
    user.require("report.view")?;
    Ok(Json(state.uploader.status().await))
}

/// Uploads what is waiting, now.
///
/// Bounded to a few batches so the request answers promptly; the background
/// loop carries on with the rest. A button that appears to hang is a button
/// people press repeatedly.
async fn upload_now(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<SyncStatus>> {
    user.require("tenant.manage")?;
    Ok(Json(state.uploader.run_now(5).await))
}

#[derive(Debug, Deserialize)]
pub struct AutomaticInput {
    pub automatic: bool,
}

async fn set_automatic(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(input): JsonBody<AutomaticInput>,
) -> ApiResult<Json<SyncStatus>> {
    user.require("tenant.manage")?;
    state.uploader.set_automatic(input.automatic).await;
    Ok(Json(state.uploader.status().await))
}
