//! The screens, served by the node itself.
//!
//! With these, any browser on the office network (or behind the tunnel, see
//! docs/RUNBOOK.md) opens GIAMS at the server's address with nothing
//! installed. The desktop app carries its own copy and doesn't need them.
//!
//! The built client (`client/dist`) lives in `web\` beside the exe; the
//! installer copies it there. In a project checkout `client/dist` is used.
//!
//! Served outside the API's licence gate on purpose: a locked system still
//! has to load the page that says it is locked.

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Router;
use tower::ServiceExt as _;
use tower_http::services::{ServeDir, ServeFile};

/// Where the built screens are, if anywhere.
pub fn folder() -> Option<PathBuf> {
    let configured = std::env::var("GIAMS_WEB_DIR").ok().filter(|p| !p.trim().is_empty());
    let candidates = configured
        .map(|p| vec![PathBuf::from(p.trim())])
        .unwrap_or_else(|| vec![PathBuf::from("web"), PathBuf::from("client/dist")]);
    candidates.into_iter().find(|dir| dir.join("index.html").is_file())
}

/// Adds the screens to the router: real files where they exist, the app's
/// own page for every other address (the client does its own routing), and a
/// plain 404 for an `/api/...` address that matched no route.
pub fn serve(app: Router, dir: &Path) -> Router {
    let files = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));

    app.fallback(move |request: Request| {
        let files = files.clone();
        async move {
            let path = request.uri().path().to_owned();
            if path.starts_with("/api/") || path == "/api" {
                return (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
                    "code": "NOT_FOUND",
                    "message": "There is no such address on this server.",
                    "reference": "WEB-404",
                })))
                    .into_response();
            }

            let mut response = match files.oneshot(request).await {
                Ok(response) => response.map(Body::new),
                Err(never) => match never {},
            };

            // file names under assets/ change whenever their content does, so
            // they can be kept forever; the page itself is checked every time
            // so an updated server is picked up on the next load
            let cache = if path.starts_with("/assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
            response
        }
    })
}
