//! The summary screen.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use giams_db::dashboard::{self, Dashboard};

use crate::{ApiResult, AppError, AppState, CurrentUser};

use super::statements::PeriodQuery;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/dashboard", get(read_dashboard))
}

/// Everything the summary screen shows, in one call.
///
/// One request rather than six, because the figures and the counts have to
/// agree with each other. Six calls landing at slightly different moments is
/// how a screen ends up showing cash that does not match the movements below
/// it, and nobody trusts it again.
async fn read_dashboard(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<PeriodQuery>,
) -> ApiResult<Json<Dashboard>> {
    user.require("report.view")?;

    if q.to < q.from {
        return Err(AppError::validation(
            "to",
            "The end of the period is before its start.",
        ));
    }

    let page = dashboard::dashboard(&state.pool, user.scope(), q.from, q.to, q.fund_id)
        .await
        .map_err(|err| match err.downcast::<giams_db::sqlx::Error>() {
            Ok(db) => AppError::Database(db),
            Err(other) => AppError::Unexpected(other),
        })?;

    Ok(Json(page))
}
