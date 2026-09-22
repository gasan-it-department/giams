//! `giams-node` — the office server.
//!
//! One static binary. It applies its own database migrations, serves the API,
//! and (once built) serves the client bundle so a browser on the LAN can reach
//! read-only dashboards without installing anything.
//!
//! Runs as a Windows Service on an always-on PC in the Accounting Office. It
//! is the single writer of ledger facts for its municipality — see
//! docs/ARCHITECTURE.md §2.

#![forbid(unsafe_code)]

mod discovery;
mod tls;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use giams_api::{AppState, NodeIdentity};
use giams_db::sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "giams=debug,tower_http=info,sqlx=warn".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;

    tracing::info!("starting giams-node {}", env!("CARGO_PKG_VERSION"));

    let pool = giams_db::connect(&config.database_url).await?;

    tracing::info!("applying database migrations");
    giams_db::migrate(&pool).await?;

    let node = bootstrap_node(&pool, &config).await?;

    // Maintenance commands run against the same database and then stop, so
    // the IT office never needs a second tool installed on the server.
    if let Some(command) = std::env::args().nth(1) {
        return run_command(&pool, node.tenant_id, &command, std::env::args().nth(2)).await;
    }

    tracing::info!(
        node = %node.code,
        municipality = %node.tenant_id,
        "this server holds the official records for its municipality"
    );

    // Report anything wrong with the audit trail at startup rather than
    // waiting for someone to ask. Tampering discovered months later is far
    // harder to investigate than tampering discovered the next morning.
    match giams_db::verify_audit_chain(&pool, node.tenant_id).await {
        Ok(problems) if problems.is_empty() => {
            tracing::info!("audit trail verified intact");
        }
        Ok(problems) => {
            tracing::error!(
                count = problems.len(),
                "AUDIT TRAIL HAS BEEN ALTERED — report this to the system administrator"
            );
            for p in &problems {
                tracing::error!(seq = p.seq, at = %p.at, problem = %p.problem);
            }
        }
        Err(e) => tracing::warn!(error = ?e, "could not verify the audit trail at startup"),
    }

    // Kept before `node` moves into the shared state below.
    let tenant_id = node.tenant_id;

    let pending = giams_sync::pending_count(&pool, node.node_id).await.unwrap_or(0);
    if pending > 0 {
        tracing::info!(pending, "changes waiting to upload to the cloud");
    }

    // This node's signing identity. Created on first run and published to the
    // database so the cloud can verify the batches it sends; without it, a
    // stolen cloud credential would be enough to forge ledger entries.
    let key = match giams_sync::NodeKey::load_or_create(&giams_sync::NodeKey::default_path()) {
        Ok(key) => {
            key.publish(&pool, node.node_id).await?;
            Some(Arc::new(key))
        }
        Err(e) => {
            // Not fatal. The office keeps its books either way; only uploading
            // stops, and the status screen says so in as many words.
            tracing::error!(error = ?e, "this node has no signing key, so nothing can be uploaded");
            None
        }
    };

    // Tell the cloud who this node is and what its key is. Failure is never
    // fatal: the office keeps its books whether or not anything is uploaded,
    // and the status screen says plainly which is happening.
    if let (Some(cloud_url), Some(secret), Some(key)) = (
        config.cloud_url.as_deref(),
        config.enrolment_secret.as_deref(),
        key.as_ref(),
    ) {
        use base64::Engine as _;

        let request = giams_sync::enrolment::Enrolment {
            tenant_id,
            tenant_code: config.tenant_code.clone(),
            municipality: config.municipality_name.clone(),
            node_id: node.node_id,
            node_code: config.node_code.clone(),
            node_name: node.name.clone(),
            public_key: base64::engine::general_purpose::STANDARD
                .encode(key.public_key().to_bytes()),
        };

        match giams_sync::enrolment::announce(cloud_url, secret, &request).await {
            Ok(enrolled) if enrolled.registered => {
                tracing::info!("this node has been registered with the cloud service");

                // The cloud starts empty, and capture only ever saw changes
                // made after it was installed. Without this the first journal
                // entry to arrive would reference a chart of accounts the
                // cloud has never been sent, and be refused.
                match sqlx::query_scalar::<_, i32>("SELECT sync_backfill($1)")
                    .bind(tenant_id)
                    .fetch_one(&pool)
                    .await
                {
                    Ok(0) => {}
                    Ok(records) => tracing::info!(
                        records,
                        "queued the existing records for their first upload"
                    ),
                    Err(e) => tracing::error!(
                        error = ?e,
                        "could not queue the existing records for upload"
                    ),
                }
            }
            Ok(_) => tracing::debug!("this node is already registered with the cloud service"),
            Err(e) => tracing::warn!(
                error = ?e,
                "could not register with the cloud service; nothing will be uploaded until this is resolved"
            ),
        }
    }


    seed_roles_and_first_administrator(&pool, node.tenant_id).await?;

    let uploader = Arc::new(giams_api::uploader::Uploader::new(
        pool.clone(),
        node.node_id,
        tenant_id,
        config.cloud_url.clone(),
        key,
        config.upload_automatically,
    ));
    giams_api::uploader::spawn(Arc::clone(&uploader));

    let licence = Arc::new(tokio::sync::RwLock::new(
        load_licence(&pool, tenant_id, &config.municipality_name).await,
    ));

    let state = AppState {
        pool,
        node: Arc::new(node),
        keys: Arc::new(giams_api::TokenKeys::from_secret(&config.token_secret)?),
        uploader,
        licence,
        login_limiter: Arc::new(giams_api::hardening::LoginLimiter::new()),
    };
    let app = giams_api::router(state).layer(TraceLayer::new_for_http());

    let addr: SocketAddr = config.bind.parse().context("GIAMS_BIND is not a valid address")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("could not listen on {addr}"))?;

    // Announce on the LAN so workstations find this server without anyone
    // typing an address. Held for the life of the process; dropping it
    // withdraws the announcement. A failure here is not fatal — clients can
    // still be pointed at the address by hand.
    let _announcement = match discovery::announce(
        &config.node_code,
        &config.municipality_name,
        tenant_id,
        addr,
    ) {
        Ok(daemon) => Some(daemon),
        Err(e) => {
            tracing::warn!(
                error = ?e,
                "could not announce on the office network; workstations will need the address entering by hand"
            );
            None
        }
    };

    // TLS when it is configured, plain HTTP when it is not. Never a hard
    // switch: a node that refused HTTP after an upgrade would take the whole
    // office offline over a setting nobody had been asked about yet.
    match tls::TlsFiles::from_env() {
        Some(files) if files.present() => {
            // The listener above was only used to claim the port; the TLS
            // server binds it itself.
            drop(listener);
            tls::serve(&files, addr, app).await?;
        }

        Some(files) => {
            tracing::error!(
                certificate = %files.certificate.display(),
                "TLS is configured but the certificate is missing; serving without encryption. \
                 Run `giams-node make-cert` to create one"
            );
            serve_plain(listener, app).await?;
        }

        None => {
            tracing::warn!(
                "serving without encryption — acceptable on a switched office LAN, but run \
                 `giams-node make-cert` before this server is reachable from anywhere else"
            );
            serve_plain(listener, app).await?;
        }
    }

    Ok(())
}

/// Plain HTTP.
///
/// The peer address is carried into the router either way, so the sign-in
/// limiter can count failures per workstation rather than per account.
async fn serve_plain(listener: tokio::net::TcpListener, app: axum::Router) -> anyhow::Result<()> {
    if let Ok(addr) = listener.local_addr() {
        tracing::info!("listening on http://{addr}");
    }

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .context("the server stopped unexpectedly")
}

struct Config {
    database_url: String,
    bind: String,
    tenant_code: String,
    node_code: String,
    municipality_name: String,
    token_secret: String,
    /// Where batches are uploaded. Absent until a municipality subscribes,
    /// which is the ordinary state for an office running on its own.
    cloud_url: Option<String>,
    /// Held by the operator of the cloud. Without it a node cannot register,
    /// which is what stops anyone replacing a municipality's signing key.
    enrolment_secret: Option<String>,
    upload_automatically: bool,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Config {
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL is not set — copy .env.example to .env")?,
            bind: std::env::var("GIAMS_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into()),
            cloud_url: std::env::var("GIAMS_CLOUD_URL").ok().filter(|u| !u.trim().is_empty()),
            enrolment_secret: std::env::var("GIAMS_CLOUD_ENROLMENT_SECRET")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            // Off unless asked for. An office that has not been told its data
            // leaves the building should not discover that it does.
            upload_automatically: std::env::var("GIAMS_UPLOAD_AUTOMATICALLY")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            tenant_code: std::env::var("GIAMS_TENANT_CODE").unwrap_or_else(|_| "GASAN".into()),
            node_code: std::env::var("GIAMS_NODE_CODE").unwrap_or_else(|_| "ACCTG-01".into()),
            municipality_name: std::env::var("GIAMS_MUNICIPALITY")
                .unwrap_or_else(|_| "Municipality of Gasan".into()),
            // No default. A signing secret that ships in the source is not a
            // secret, and every installation must have its own.
            token_secret: std::env::var("GIAMS_TOKEN_SECRET").context(
                "GIAMS_TOKEN_SECRET is not set — generate one with `openssl rand -base64 48`",
            )?,
        })
    }
}

/// Finds this node's identity, creating the municipality and node rows on a
/// fresh installation.
///
/// The unique index `node_one_primary_per_tenant` means a second server
/// pointed at the same database cannot claim to be primary — it is refused by
/// the database rather than quietly becoming a second writer.
async fn bootstrap_node(pool: &PgPool, config: &Config) -> anyhow::Result<NodeIdentity> {
    let tenant_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO tenant (code, name)
             VALUES ($1, $2)
        ON CONFLICT (code) DO UPDATE SET name = excluded.name
          RETURNING id
        "#,
    )
    .bind(&config.tenant_code)
    .bind(&config.municipality_name)
    .fetch_one(pool)
    .await
    .context("could not establish the municipality record")?;

    let (node_id, name): (Uuid, String) = sqlx::query_as(
        r#"
        INSERT INTO node (tenant_id, code, name, public_key, is_primary)
             VALUES ($1, $2, $3, '\x'::bytea, true)
        ON CONFLICT (tenant_id, code) DO UPDATE SET last_seen_at = now()
          RETURNING id, name
        "#,
    )
    .bind(tenant_id)
    .bind(&config.node_code)
    .bind(format!("{} accounting server", config.municipality_name))
    .fetch_one(pool)
    .await
    .context("could not establish this server's identity")?;

    Ok(NodeIdentity {
        node_id,
        tenant_id,
        code: config.node_code.clone(),
        name,
    })
}

/// Seeds the standard roles, and creates the first administrator on a fresh
/// installation.
///
/// The initial password is generated, never defaulted, and printed once to the
/// server log. A shipped default password is the single most common way a
/// system like this is compromised, and it is always still in place years
/// later. The account is flagged to force a change on first sign-in.
async fn seed_roles_and_first_administrator(pool: &PgPool, tenant_id: Uuid) -> anyhow::Result<()> {
    sqlx::query("SELECT seed_default_roles($1)")
        .bind(tenant_id)
        .execute(pool)
        .await
        .context("could not set up the default roles")?;

    // The statutory funds. Fixed by law and identical for every LGU, so this
    // is a complete seed rather than a starting point.
    sqlx::query("SELECT seed_funds($1)")
        .bind(tenant_id)
        .execute(pool)
        .await
        .context("could not set up the standard funds")?;

    // The offices appropriations are made to and obligations charged against.
    // A starting roster; offices are created and merged by ordinance, so this
    // never touches one the municipality has since edited.
    let offices: i32 = sqlx::query_scalar("SELECT seed_offices($1)")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .context("could not set up the municipal offices")?;

    if offices > 0 {
        tracing::info!(offices, "seeded the municipal offices");
    }

    // Starter chart of accounts. Does nothing once accounts exist, so it never
    // fights a chart the Accounting Office has since curated.
    let seeded: i32 = sqlx::query_scalar("SELECT seed_gasan_chart_of_accounts($1)")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .context("could not set up the starter chart of accounts")?;

    if seeded > 0 {
        tracing::info!(
            accounts = seeded,
            "seeded the Accounting Office chart of accounts"
        );
    }

    // Mandatory deductions. Depends on the chart of accounts above for the
    // liability accounts they credit.
    let rates: i32 = sqlx::query_scalar("SELECT seed_deduction_types($1)")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .context("could not set up the mandatory deductions")?;

    if rates > 0 {
        tracing::warn!(
            rates,
            "seeded starting deduction rates -- VERIFY these against current \
             BIR, GSIS, PhilHealth and HDMF issuances before recording real vouchers"
        );
    }

    let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM app_user WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

    if existing > 0 {
        return Ok(());
    }

    let password = generate_initial_password();
    let hash = giams_api::auth::password::hash_password(&password)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let user_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO app_user (tenant_id, username, full_name, position_title,
                              password_hash, must_change_password)
             VALUES ($1, 'administrator', 'System Administrator',
                     'Information Technology', $2, true)
          RETURNING id
        "#,
    )
    .bind(tenant_id)
    .bind(&hash)
    .fetch_one(pool)
    .await
    .context("could not create the first administrator")?;

    sqlx::query(
        "INSERT INTO user_role (tenant_id, user_id, role_id)
         SELECT $1, $2, id FROM role WHERE tenant_id = $1 AND code = 'ADMIN'",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    tracing::warn!(
        "\n\n\
         ┌────────────────────────────────────────────────────────────────┐\n\
         │  FIRST-TIME SETUP                                              │\n\
         │  An administrator account has been created.                    │\n\
         │                                                                │\n\
         │    Username:  administrator                                    │\n\
         │    Password:  {password:<49}│\n\
         │                                                                │\n\
         │  This is shown once and is not recoverable. Sign in now and    │\n\
         │  change it — you will be prompted to.                          │\n\
         └────────────────────────────────────────────────────────────────┘\n"
    );

    Ok(())
}

/// Maintenance commands, run from the server's console.
///
/// Deliberately few. Anything that changes the ledger belongs in the API,
/// where it is subject to permissions and separation of duties; only the two
/// things nobody can do through the API — because they cannot sign in — are
/// here.
async fn run_command(
    pool: &PgPool,
    tenant_id: Uuid,
    command: &str,
    argument: Option<String>,
) -> anyhow::Result<()> {
    match command {
        "reset-password" => {
            let username = argument.context(
                "which account? usage: giams-node reset-password <username>",
            )?;
            reset_password(pool, tenant_id, &username).await
        }
        "backup" => backup(argument.as_deref()),

        "import-opening-balances" => {
            let path = argument.context(
                "which file? usage: giams-node import-opening-balances <file.csv> \
                 <fund code> <year>",
            )?;
            let fund = std::env::args().nth(3).context("which fund? give its code, e.g. 01")?;
            let year = std::env::args()
                .nth(4)
                .context("which year? give the fiscal year, e.g. 2026")?
                .parse::<i16>()
                .context("the year is not a number")?;

            import_opening_balances(pool, tenant_id, &path, &fund, year).await
        }

        "make-cert" => {
            // Every remaining argument is another name or address the server
            // should answer to.
            let names: Vec<String> = std::env::args().skip(2).collect();
            tls::make_certificate(&names, std::path::Path::new("tls")).map(|_| ())
        }

        "verify-audit" => {
            let problems = giams_db::verify_audit_chain(pool, tenant_id).await?;
            if problems.is_empty() {
                println!("The audit trail is intact.");
            } else {
                println!("THE AUDIT TRAIL HAS BEEN ALTERED. {} problems:", problems.len());
                for p in &problems {
                    println!("  entry {} at {}: {}", p.seq, p.at, p.problem);
                }
                std::process::exit(1);
            }
            Ok(())
        }
        other => {
            anyhow::bail!(
                "there is no command called `{other}`. \
                 Available: reset-password <username>, verify-audit, \
                 backup [folder], make-cert [name or address ...], \
                 import-opening-balances <file.csv> <fund code> <year>"
            )
        }
    }
}

/// Brings the legacy opening balances in from a spreadsheet export.
///
/// ```text
/// giams-node import-opening-balances balances.csv 01 2026
/// ```
///
/// The file is `account_code, debit, credit`, with or without a header row.
/// Amounts may carry the thousands separators a spreadsheet exports —
/// `1,250,000.00` is what the office's own file will contain, and refusing it
/// would mean asking somebody to reformat 263 rows by hand.
///
/// Nothing is written unless the whole file is valid and balances; the
/// database function does all of that in one transaction. This half only reads
/// the file and reports what happened.
async fn import_opening_balances(
    pool: &PgPool,
    tenant_id: Uuid,
    path: &str,
    fund_code: &str,
    fiscal_year: i16,
) -> anyhow::Result<()> {
    use anyhow::{bail, Context};

    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {path}"))?;

    let mut lines = Vec::new();

    for (number, row) in text.lines().enumerate() {
        let row = row.trim();
        if row.is_empty() {
            continue;
        }

        let fields = split_csv_row(row);

        // A header row, in whatever case the spreadsheet wrote it.
        if number == 0 && fields[0].to_lowercase().contains("account") {
            continue;
        }

        if fields.len() < 3 {
            bail!(
                "line {} has {} column(s); it needs three: account code, debit, credit",
                number + 1,
                fields.len()
            );
        }

        let debit = clean_amount(&fields[1]).with_context(|| {
            format!("line {}: `{}` is not an amount", number + 1, fields[1])
        })?;
        let credit = clean_amount(&fields[2]).with_context(|| {
            format!("line {}: `{}` is not an amount", number + 1, fields[2])
        })?;

        lines.push(serde_json::json!({
            "account_code": fields[0].clone(),
            "debit":  debit,
            "credit": credit,
        }));
    }

    if lines.is_empty() {
        bail!("{path} contains no rows");
    }

    let fund_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM fund WHERE tenant_id = $1 AND code = $2",
    )
    .bind(tenant_id)
    .bind(fund_code)
    .fetch_optional(pool)
    .await?;

    let fund_id = fund_id
        .with_context(|| format!("there is no fund with code `{fund_code}`"))?;

    let user_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM app_user WHERE tenant_id = $1 ORDER BY created_at LIMIT 1")
            .bind(tenant_id)
            .fetch_optional(pool)
            .await?;

    println!("Reading {} balance(s) from {path}…", lines.len());

    let (_, jev_no, count, total): (Uuid, String, i32, rust_decimal::Decimal) = sqlx::query_as(
        "SELECT entry_id, jev_no, lines, total
           FROM import_opening_balances($1, $2, $3::smallint, $4, $5)",
    )
    .bind(tenant_id)
    .bind(fund_id)
    .bind(fiscal_year)
    .bind(serde_json::Value::Array(lines))
    .bind(user_id)
    .fetch_one(pool)
    .await
    .context("the opening balances were not recorded")?;

    println!();
    println!("┌────────────────────────────────────────────────────────────────┐");
    println!("│  OPENING BALANCES RECORDED                                     │");
    println!("│                                                                │");
    println!("│    Entry:    {:<50}│", jev_no);
    println!("│    Accounts: {:<50}│", count.to_string());
    println!("│    Total:    {:<50}│", format!("{total:.2} on each side"));
    println!("│                                                                │");
    println!("│  Check the trial balance against the legacy books before       │");
    println!("│  recording anything else.                                      │");
    println!("└────────────────────────────────────────────────────────────────┘");
    println!();

    Ok(())
}

/// Splits one CSV row, honouring quoted fields.
///
/// A spreadsheet writes `1-01-02-010,"1,250,000.00",0.00`, so splitting on
/// every comma tears the thousands separators out of the amount and shifts
/// every column after it. Getting this wrong is not a formatting nuisance: a
/// row whose debit column is empty would have its *credit* read as a debit,
/// and the import would balance while being exactly backwards.
fn split_csv_row(row: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = row.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                // A doubled quote inside a quoted field is one literal quote.
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }

    fields.push(current.trim().to_string());
    fields
}

/// Reads one money column as the exact decimal string the database expects.
///
/// Thousands separators are dropped, an empty cell is zero, and anything else
/// is refused rather than guessed at. Deliberately never parsed into a float
/// on the way through — the string goes to the database as written.
fn clean_amount(field: &str) -> anyhow::Result<String> {
    let cleaned: String = field
        .chars()
        .filter(|c| !matches!(c, ',' | ' ' | '\u{a0}' | '\u{20b1}'))
        .collect();

    let cleaned = cleaned.trim();

    if cleaned.is_empty() || cleaned == "-" {
        return Ok("0.00".into());
    }

    // Parenthesised negatives are how a spreadsheet writes them, and they have
    // no meaning in an opening balance — the column already says which side it
    // is. Refused rather than silently read as positive.
    if cleaned.starts_with('(') {
        anyhow::bail!("amounts in brackets are not accepted; put the figure in the other column");
    }

    if !cleaned.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
        anyhow::bail!("not a number");
    }

    // Parsed only to confirm it is a well-formed decimal; the original string
    // is what travels onward.
    cleaned
        .parse::<rust_decimal::Decimal>()
        .map_err(|_| anyhow::anyhow!("not a number"))?;

    Ok(cleaned.to_string())
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn a_quoted_thousands_separator_does_not_shift_the_columns() {
        let fields = split_csv_row("1-01-02-010,\"1,250,000.00\",0.00");
        assert_eq!(fields, vec!["1-01-02-010", "1,250,000.00", "0.00"]);
    }

    #[test]
    fn an_empty_debit_stays_in_its_own_column() {
        // The bug this guards against: reading the credit as a debit, which
        // balances perfectly and is exactly backwards.
        let fields = split_csv_row("2-01-01-010,,90500.75");
        assert_eq!(fields, vec!["2-01-01-010", "", "90500.75"]);
        assert_eq!(clean_amount(&fields[1]).unwrap(), "0.00");
        assert_eq!(clean_amount(&fields[2]).unwrap(), "90500.75");
    }

    #[test]
    fn separators_and_currency_marks_are_stripped() {
        assert_eq!(clean_amount("1,250,000.00").unwrap(), "1250000.00");
        assert_eq!(clean_amount(" 340500.75 ").unwrap(), "340500.75");
        assert_eq!(clean_amount("").unwrap(), "0.00");
        assert_eq!(clean_amount("-").unwrap(), "0.00");
    }

    #[test]
    fn anything_that_is_not_a_number_is_refused_rather_than_guessed() {
        for bad in ["abc", "1.2.3", "(500.00)", "12x"] {
            assert!(clean_amount(bad).is_err(), "{bad} was accepted");
        }
    }
}

/// Writes a complete copy of the municipality's records to a file.
///
/// Wraps `pg_dump` rather than reimplementing it. A hand-rolled exporter that
/// misses a table, a sequence, or a constraint produces a backup that restores
/// into something subtly different from what was backed up — and that is only
/// discovered on the day it is restored, which is the worst possible day.
///
/// The custom format is used because it restores selectively and compresses,
/// and because `pg_restore` refuses a file it does not recognise rather than
/// half-loading it.
fn backup(folder: Option<&str>) -> anyhow::Result<()> {
    use anyhow::{bail, Context};

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;

    let folder = folder.unwrap_or("backups");
    std::fs::create_dir_all(folder)
        .with_context(|| format!("could not create the backup folder at {folder}"))?;

    // Sorts chronologically as text, which is what makes a folder of these
    // readable a year later.
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
    let path = std::path::Path::new(folder).join(format!("giams-{stamp}.dump"));

    println!("Backing up to {}…", path.display());

    let output = std::process::Command::new("pg_dump")
        .arg("--format=custom")
        .arg("--no-owner")
        .arg("--no-privileges")
        .arg("--file")
        .arg(&path)
        .arg(&database_url)
        .output()
        .context(
            "could not run pg_dump — it comes with PostgreSQL and must be on the PATH",
        )?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        // Removed, so a failed backup never leaves a file that looks like one.
        let _ = std::fs::remove_file(&path);
        bail!("the backup did not complete: {}", detail.trim());
    }

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if size == 0 {
        let _ = std::fs::remove_file(&path);
        bail!("the backup file came out empty, so it has been removed");
    }

    println!();
    println!("┌────────────────────────────────────────────────────────────────┐");
    println!("│  BACKUP COMPLETE                                               │");
    println!("│                                                                │");
    println!("│    File:  {:<53}│", path.display().to_string().chars().take(53).collect::<String>());
    println!("│    Size:  {:<53}│", format!("{:.1} MB", size as f64 / 1_048_576.0));
    println!("│                                                                │");
    println!("│  Copy this to a drive that is not this computer. A backup      │");
    println!("│  stored only on the server it came from is not a backup.       │");
    println!("│                                                                │");
    println!("│  To restore:  pg_restore --clean --if-exists -d <database>     │");
    println!("│               \"<file>\"                                         │");
    println!("└────────────────────────────────────────────────────────────────┘");
    println!();

    Ok(())
}

/// Issues a new password for an account that has been locked out.
///
/// The only way back in when the first-run password is lost, and the reason
/// this command exists at all. The new password must be changed at the next
/// sign-in, and every session the account already holds is revoked — a reset
/// that left an attacker's session alive would be worse than no reset.
///
/// Recorded in the audit trail with no actor, because whoever ran it had
/// console access to the server rather than an account in the system. That
/// distinction matters to an auditor reading the trail later.
async fn reset_password(pool: &PgPool, tenant_id: Uuid, username: &str) -> anyhow::Result<()> {
    let user_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM app_user WHERE tenant_id = $1 AND username = $2")
            .bind(tenant_id)
            .bind(username)
            .fetch_optional(pool)
            .await
            .context("could not look the account up")?;

    let user_id = user_id.with_context(|| {
        format!("there is no account called `{username}` in this municipality")
    })?;

    let password = generate_initial_password();
    let hash = giams_api::auth::password::hash_password(&password)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE app_user
            SET password_hash = $3, must_change_password = true,
                failed_attempts = 0, locked_until = NULL
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(&hash)
    .execute(&mut *tx)
    .await
    .context("could not set the new password")?;

    let revoked = sqlx::query(
        "UPDATE user_session
            SET revoked_at = now(), revoked_reason = 'password_changed'
          WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .context("could not sign the account out of its existing sessions")?
    .rows_affected();

    sqlx::query(
        r#"
        INSERT INTO audit_log (tenant_id, actor_label, action, entity, entity_id, after)
             VALUES ($1, 'server console', 'password.reset', 'app_user', $2, $3)
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(serde_json::json!({ "username": username, "sessions_revoked": revoked }))
    .execute(&mut *tx)
    .await
    .context("could not record the reset in the audit trail")?;

    tx.commit().await?;

    // Padded to a fixed width so the box closes, whatever the name's length.
    let signed_out = format!("{revoked} existing session(s) were signed out.");

    println!(
        "\n\
         ┌────────────────────────────────────────────────────────────────┐\n\
         │  PASSWORD RESET                                                │\n\
         │                                                                │\n\
         │    Username:  {username:<49}│\n\
         │    Password:  {password:<49}│\n\
         │                                                                │\n\
         │  Shown once and not recoverable. It must be changed at the     │\n\
         │  next sign-in. {signed_out:<48}│\n\
         └────────────────────────────────────────────────────────────────┘\n"
    );

    Ok(())
}

/// A readable, high-entropy initial password.
///
/// Grouped into blocks because somebody has to type this from a screen, and an
/// unreadable password gets written on a sticky note instead.
fn generate_initial_password() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();

    (0..4)
        .map(|_| {
            (0..5)
                .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("-")
}

use giams_db::sqlx;


/// Reads the licence on file and works out what it permits.
///
/// Every failure here lands on the safe side for the *municipality*, not for
/// the vendor: a licence that cannot be read leaves the system unrestricted,
/// because the alternative is an office locked out of its own books by a bug
/// in licence handling. The subscription is enforced when there is a licence
/// saying so, never by the absence of one being read successfully.
async fn load_licence(
    pool: &PgPool,
    tenant_id: Uuid,
    municipality: &str,
) -> giams_licence::LicenceGuard {
    use giams_licence::{clock, token, Licence, LicenceGuard};

    // Recorded first, so a clock that has been set back is noticed even on an
    // installation with no licence at all.
    let clock_state = match clock::observe(pool, tenant_id, chrono::Utc::now()).await {
        Ok(state) => {
            match state {
                clock::ClockState::Trusted => {}
                clock::ClockState::MovedBackwards { by_days } => tracing::warn!(
                    by_days,
                    "this computer's clock reads earlier than a date already seen"
                ),
                clock::ClockState::Tampered => tracing::error!(
                    "this installation's record of the date has been altered"
                ),
            }
            state
        }
        Err(e) => {
            tracing::warn!(error = ?e, "could not check the clock; treating it as trustworthy");
            clock::ClockState::Trusted
        }
    };

    let stored: Option<String> = sqlx::query_scalar("SELECT token FROM licence WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = ?e, "could not read the licence on file");
            None
        })
        .flatten();

    let Some(stored) = stored else {
        tracing::info!(
            municipality,
            "no subscription is installed; running as a local installation"
        );
        return LicenceGuard::with_clock(Licence::unrestricted(tenant_id), clock_state);
    };

    let claims = match token::authority_public_key().and_then(|key| token::verify(&stored, &key)) {
        Ok(claims) => claims,
        Err(e) => {
            // Loud, because a licence that stops verifying is either tampering
            // or a build with the wrong authority key, and both need a person.
            tracing::error!(error = ?e, "the licence on file could not be verified");
            return LicenceGuard::with_clock(Licence::unrestricted(tenant_id), clock_state);
        }
    };

    if !giams_licence::fingerprint::matches(claims.fingerprint.as_deref()) {
        tracing::error!("the licence on file was issued for a different computer");
        return LicenceGuard::with_clock(Licence::unrestricted(tenant_id), clock_state);
    }

    tracing::info!(
        plan = %claims.plan,
        expires = %claims.expires_at,
        seats = claims.seats,
        "subscription in force"
    );

    LicenceGuard::with_clock(
        Licence {
            tenant_id,
            plan: claims.plan,
            seats: claims.seats,
            expires_at: Some(claims.expires_at),
            grace_days: claims.grace_days,
        },
        clock_state,
    )
}
