//! `giams-cloud` — the receiving service.
//!
//! Accepts signed batches from municipal nodes and applies them to a
//! consolidated database. Deliberately small: it receives, it verifies, it
//! applies. It does not originate ledger facts, and it has no route that can.
//!
//! That asymmetry is the design (docs/ARCHITECTURE.md §2). Replication is one
//! directional — node to cloud, never back — which removes the entire class of
//! bidirectional merge conflicts, and means a compromised cloud cannot alter a
//! municipality's books. It can only fail to have a copy of them.

#![forbid(unsafe_code)]

use std::net::SocketAddr;

use anyhow::Context;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use giams_db::sqlx::PgPool;
use giams_sync::batch::SIGNATURE_HEADER;
use giams_sync::enrolment::{Enrolment, ENROLMENT_HEADER};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "giams=debug,sqlx=warn".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("GIAMS_CLOUD_DATABASE_URL")
        .context("GIAMS_CLOUD_DATABASE_URL is not set")?;
    let bind = std::env::var("GIAMS_CLOUD_BIND").unwrap_or_else(|_| "127.0.0.1:8788".into());

    // Issuing a licence is a command, not a service. Run on the machine that
    // holds the authority's private key, which is the only machine that can.
    if std::env::args().nth(1).as_deref() == Some("issue-licence") {
        return issue_licence();
    }

    let pool = giams_db::connect(&database_url).await?;

    tracing::info!("applying database migrations");
    giams_db::migrate(&pool).await?;

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/sync/enrol", post(enrol))
        .route("/api/sync/batches", post(receive_batch))
        .with_state(pool)
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = bind.parse().context("GIAMS_CLOUD_BIND is not a valid address")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("could not listen on {addr}"))?;

    tracing::info!("giams-cloud listening on http://{addr}");
    axum::serve(listener, app).await.context("the service stopped unexpectedly")?;

    Ok(())
}

async fn health(State(pool): State<PgPool>) -> Response {
    match giams_db::ping(&pool).await {
        Ok(()) => Json(serde_json::json!({
            "service":  "giams-cloud",
            "database": "reachable",
        }))
        .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "service": "giams-cloud", "database": "unreachable" })),
        )
            .into_response(),
    }
}

/// Registers a node and its signing key.
///
/// Gated by a secret the cloud's operator holds, because this endpoint writes
/// the very key every later signature is checked against — see
/// `giams_sync::enrolment` for why an open version of this would defeat the
/// whole scheme.
async fn enrol(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(request): Json<Enrolment>,
) -> Response {
    let Ok(expected) = std::env::var("GIAMS_CLOUD_ENROLMENT_SECRET") else {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "this service is not accepting new nodes",
        );
    };

    let presented = headers.get(ENROLMENT_HEADER).and_then(|v| v.to_str().ok()).unwrap_or("");

    // Constant time: a byte-at-a-time comparison here leaks the secret to
    // anyone patient enough to measure the difference.
    if !constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
        return refuse(StatusCode::FORBIDDEN, "this node is not permitted to enrol");
    }

    match giams_sync::enrolment::enrol(&pool, &request).await {
        Ok(enrolled) => {
            if enrolled.registered {
                tracing::info!(node = %enrolled.node_id, "a node enrolled");
            }
            Json(enrolled).into_response()
        }
        Err(e) => {
            tracing::warn!(error = ?e, "an enrolment was refused");
            refuse(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string())
        }
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

/// Receives one signed batch.
///
/// Takes the raw body rather than a parsed struct, because the signature
/// covers the bytes as sent. Verifying against a re-serialisation would check
/// a signature over something the node never signed.
async fn receive_batch(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    use base64::Engine as _;

    let Some(header) = headers.get(SIGNATURE_HEADER) else {
        return refuse(StatusCode::BAD_REQUEST, "the batch is not signed");
    };

    let Ok(signature) = header
        .to_str()
        .map_err(|_| ())
        .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| ()))
    else {
        return refuse(StatusCode::BAD_REQUEST, "the batch signature is not readable");
    };

    match giams_sync::receiver::receive(&pool, &body, &signature).await {
        Ok(receipt) => Json(receipt).into_response(),

        Err(e) => {
            // Logged with the full chain here, where an operator can see it;
            // the node is told only the top-level reason. A receiver that
            // narrates its internals to an unauthenticated caller is telling
            // an attacker how to get past it.
            tracing::warn!(error = ?e, "a batch was refused");
            refuse(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string())
        }
    }
}

fn refuse(status: StatusCode, reason: &str) -> Response {
    (status, Json(serde_json::json!({ "refused": reason }))).into_response()
}


/// Mints a licence key.
///
/// ```text
/// giams-cloud issue-licence <tenant-id> <municipality> <plan> <seats> <months>
///                           [start YYYY-MM-DD] [fingerprint]
/// ```
///
/// The start date matters for renewals: a municipality that renews a month
/// early should not forfeit the month it already paid for, so the new licence
/// starts when the old one ends rather than today.
///
/// The private key is read from `GIAMS_LICENCE_AUTHORITY_KEY` — a path, not
/// the key itself, so it never appears in a shell history or a process list.
/// Nothing about issuance touches the database: a licence is a signed
/// assertion, and the authority needs no record of it to be believed.
fn issue_licence() -> anyhow::Result<()> {
    use anyhow::{bail, Context};
    use chrono::{Duration, Utc};
    use ed25519_dalek::SigningKey;
    use giams_licence::token::{issue, Claims};

    let args: Vec<String> = std::env::args().skip(2).collect();

    if args.len() < 5 {
        bail!(
            "usage: giams-cloud issue-licence <tenant-id> <municipality> <plan>              <seats> <months> [start YYYY-MM-DD] [fingerprint]"
        );
    }

    let key_path = std::env::var("GIAMS_LICENCE_AUTHORITY_KEY")
        .context("GIAMS_LICENCE_AUTHORITY_KEY is not set -- it is the path to the private key")?;

    let seed = std::fs::read(&key_path)
        .with_context(|| format!("could not read the authority key at {key_path}"))?;
    let seed: [u8; 32] = seed
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("the authority key is not 32 bytes"))?;

    let signing = SigningKey::from_bytes(&seed);

    let months: i64 = args[4].parse().context("the number of months is not a number")?;
    if months <= 0 {
        bail!("a licence must run for at least one month");
    }

    let now = Utc::now();

    let starts = match args.get(5) {
        Some(date) => date
            .parse::<chrono::NaiveDate>()
            .context("the start date must be written as YYYY-MM-DD")?
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time")
            .and_utc(),
        None => now,
    };

    let claims = Claims {
        tenant_id: args[0].parse().context("that is not a valid municipality id")?,
        municipality: args[1].clone(),
        plan: args[2].clone(),
        seats: args[3].parse().context("the number of seats is not a number")?,
        issued_at: now,
        not_before: starts,
        expires_at: starts + Duration::days(months * 30),
        // Two weeks. Long enough to survive a payment crossing a holiday, which
        // in a municipal treasury is the normal case rather than the exception.
        grace_days: 14,
        fingerprint: args.get(6).cloned(),
        features: vec![],
        people: vec![],
    };

    let token = issue(&claims, &signing)?;

    println!();
    println!("Licence for {}", claims.municipality);
    println!("  plan:    {} ({} seats)", claims.plan, claims.seats);
    println!("  runs:    {} to {}",
             claims.not_before.format("%-d %B %Y"),
             claims.expires_at.format("%-d %B %Y"));
    println!("  grace:   {} days after expiry", claims.grace_days);
    println!();
    println!("{token}");
    println!();

    Ok(())
}
