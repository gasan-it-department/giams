//! HTTP surface.
//!
//! Routers, authentication, role checks, and the error taxonomy. No accounting
//! logic lives here — this layer translates between HTTP and the domain, and
//! is responsible for making sure nothing technical reaches a user.

#![forbid(unsafe_code)]

pub mod auth;
pub mod error;
pub mod extract;
pub mod hardening;
pub mod routes;
pub mod licensing;
pub mod uploader;

use axum::Router;
use giams_db::sqlx::PgPool;
use std::sync::Arc;

pub use auth::{CurrentUser, TokenKeys};
pub use error::{ApiResult, AppError};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub node: Arc<NodeIdentity>,
    pub keys: Arc<TokenKeys>,
    pub uploader: Arc<uploader::Uploader>,
    /// Replaceable while the server runs, so a renewal takes effect without a
    /// restart.
    pub licence: licensing::SharedLicence,
    /// Counts failed sign-ins per source address.
    pub login_limiter: Arc<hardening::LoginLimiter>,
}

/// Who this server is. Established at startup from configuration and the
/// `node` table; every ledger fact it writes is attributed here.
#[derive(Debug, Clone)]
pub struct NodeIdentity {
    pub node_id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub code: String,
    pub name: String,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .merge(routes::reference::router())
        .merge(routes::budget::router())
        .merge(routes::books::router())
        .merge(routes::statements::router())
        .merge(routes::export::router())
        .merge(routes::dashboard::router())
        .merge(routes::sync::router())
        .merge(routes::vouchers::router())
        .merge(routes::reconciliation::router())
        .merge(routes::licence::router())
        // Applied last so it wraps every route above it. Enforcement is a
        // layer rather than a per-route check: the route somebody forgets to
        // guard is always the one that matters.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            licensing::enforce,
        ))
        // Outside everything, so even a refused request carries them.
        .layer(axum::middleware::from_fn(hardening::security_headers))
        // A body larger than this is refused before it is read into memory.
        .layer(axum::extract::DefaultBodyLimit::max(hardening::MAX_BODY_BYTES))
        .with_state(state)
}
