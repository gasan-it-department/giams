//! Health and readiness.
//!
//! Used by the desktop client to decide whether it is working against the
//! office node or falling back to offline drafting, so the shape of this
//! response is part of the offline behaviour, not just an ops convenience.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::{ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/health/audit", get(audit_integrity))
}

#[derive(Serialize)]
pub struct Health {
    /// Always `"giams-node"`. Lets the client confirm it found the right
    /// service when discovering nodes over the LAN.
    service: &'static str,
    version: &'static str,
    database: &'static str,
    node_code: String,
    municipality_id: uuid::Uuid,
    server_time: chrono::DateTime<chrono::Utc>,
}

async fn health(State(state): State<AppState>) -> ApiResult<Json<Health>> {
    giams_db::ping(&state.pool)
        .await
        .map_err(crate::AppError::Unexpected)?;

    Ok(Json(Health {
        service: "giams-node",
        version: env!("CARGO_PKG_VERSION"),
        database: "reachable",
        node_code: state.node.code.clone(),
        municipality_id: state.node.tenant_id,
        server_time: chrono::Utc::now(),
    }))
}

#[derive(Serialize)]
pub struct AuditIntegrity {
    intact: bool,
    /// Plain language, because this is shown to an administrator and
    /// potentially quoted to an auditor.
    summary: String,
    problems: Vec<giams_db::AuditChainProblem>,
}

/// Walks the whole audit chain and reports any tampering.
async fn audit_integrity(State(state): State<AppState>) -> ApiResult<Json<AuditIntegrity>> {
    let problems = giams_db::verify_audit_chain(&state.pool, state.node.tenant_id)
        .await
        .map_err(crate::AppError::Unexpected)?;

    let summary = if problems.is_empty() {
        "The audit trail is complete and unaltered.".to_string()
    } else {
        format!(
            "The audit trail has {} record(s) that do not match what was originally written. \
             This needs to be reported to your system administrator.",
            problems.len()
        )
    };

    Ok(Json(AuditIntegrity {
        intact: problems.is_empty(),
        summary,
        problems,
    }))
}
