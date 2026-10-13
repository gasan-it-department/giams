//! The admin console: user accounts, and licence profiles.
//!
//! Every route here re-checks its permission. The client hides the section
//! from anyone without it, but that is a convenience; this is the control.

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Months, NaiveDate, Utc};
use giams_db::accounts::{self, Account, NewAccount, RoleOption};
use giams_db::licensees::{
    self, IssueRecord, LicenseeDetail, LicenseeSummary, NewLicensee, NewPerson,
};
use giams_licence::token::{self, Claims, LicensedPerson};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::password::{generate_one_time_password, hash_password};
use crate::error::{as_sentence, database_error};
use crate::extract::Json as JsonBody;
use crate::{ApiResult, AppError, AppState, CurrentUser};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/roles", get(list_roles))
        .route("/api/admin/users", get(list_users).post(create_user))
        .route("/api/admin/users/{id}/password", post(reset_password))
        .route("/api/admin/users/{id}/active", post(set_active))
        .route("/api/admin/licensing", get(capability))
        .route("/api/admin/licensees", get(list_licensees).post(save_licensee))
        .route("/api/admin/licensees/generate", post(generate_licence))
        .route("/api/admin/licensees/{id}", get(get_licensee))
        .route("/api/admin/licensees/{id}/renew", post(renew))
}

use crate::licensing::GRACE_DAYS;

// ── Accounts ─────────────────────────────────────────────────────────────────

async fn list_roles(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<RoleOption>>> {
    user.require("user.manage")?;
    let roles = accounts::list_roles(&state.pool, user.scope()).await.map_err(database_error)?;
    Ok(Json(roles))
}

async fn list_users(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Account>>> {
    user.require("user.manage")?;
    let rows = accounts::list_accounts(&state.pool, user.scope()).await.map_err(database_error)?;
    Ok(Json(rows))
}

/// An account together with the one-time password it was just given.
///
/// The password is in this response and nowhere else — not stored, not
/// logged. Whoever reads it hands it over, and the account must change it at
/// the first sign-in.
#[derive(Serialize)]
struct AccountWithPassword {
    account: Account,
    password: String,
}

async fn create_user(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(mut input): JsonBody<NewAccount>,
) -> ApiResult<Json<AccountWithPassword>> {
    user.require("user.manage")?;

    input.username = normalise_username(&input.username);

    let mut fields = BTreeMap::new();
    check_username(&mut fields, "username", &input.username);
    if input.full_name.trim().is_empty() {
        fields.insert("full_name".into(), "Enter the person's full name.".into());
    }
    if input.role.trim().is_empty() {
        fields.insert("role".into(), "Choose a role.".into());
    }
    if !fields.is_empty() {
        return Err(AppError::Validation { fields });
    }

    let password = generate_one_time_password();
    let hash = hash_password(&password).map_err(AppError::Unexpected)?;

    let account =
        accounts::create_account(&state.pool, user.scope(), &user.username, input, &hash)
            .await
            .map_err(database_error)?;

    Ok(Json(AccountWithPassword { account, password }))
}

async fn reset_password(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AccountWithPassword>> {
    user.require("user.manage")?;

    if id == user.user_id {
        return Err(AppError::Rule {
            code: "RESET_OWN_PASSWORD",
            message: "You cannot reset your own password from here.".into(),
            hint: Some("Use Change password on your own account instead.".into()),
        });
    }

    let password = generate_one_time_password();
    let hash = hash_password(&password).map_err(AppError::Unexpected)?;

    let account =
        accounts::reset_account_password(&state.pool, user.scope(), &user.username, id, &hash)
            .await
            .map_err(database_error)?
            .ok_or(AppError::NotFound { entity: "account", label: "That account".into() })?;

    Ok(Json(AccountWithPassword { account, password }))
}

#[derive(Deserialize)]
struct ActiveInput {
    active: bool,
}

async fn set_active(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    JsonBody(input): JsonBody<ActiveInput>,
) -> ApiResult<Json<Account>> {
    user.require("user.manage")?;

    if id == user.user_id && !input.active {
        return Err(AppError::Rule {
            code: "DEACTIVATE_SELF",
            message: "You cannot switch off your own account while signed in to it.".into(),
            hint: Some("Ask another administrator to do it.".into()),
        });
    }

    let account =
        accounts::set_account_active(&state.pool, user.scope(), &user.username, id, input.active)
            .await
            .map_err(database_error)?
            .ok_or(AppError::NotFound { entity: "account", label: "That account".into() })?;

    Ok(Json(account))
}

// ── Licensing ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ThisServer {
    tenant_id: Uuid,
    tenant_code: String,
    municipality: String,
}

#[derive(Serialize)]
struct Capability {
    /// Whether this server holds the authority's key and can sign.
    can_issue: bool,
    /// Why not, in words, when it cannot.
    reason: Option<String>,
    this_server: ThisServer,
}

async fn capability(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Capability>> {
    user.require("licence.issue")?;

    let (tenant_code, municipality) = licensees::tenant_identity(&state.pool, state.node.tenant_id)
        .await
        .map_err(database_error)?;

    Ok(Json(Capability {
        can_issue: state.issuer.is_some(),
        reason: state.issuer.is_none().then(|| {
            "This server does not hold the licensing key, so it cannot issue licence keys. \
             Keys are issued from the licensing authority's own server."
                .into()
        }),
        this_server: ThisServer { tenant_id: state.node.tenant_id, tenant_code, municipality },
    }))
}

async fn list_licensees(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<LicenseeSummary>>> {
    user.require("licence.issue")?;
    let rows = licensees::list_licensees(&state.pool, user.scope()).await.map_err(database_error)?;
    Ok(Json(rows))
}

async fn get_licensee(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<LicenseeDetail>> {
    user.require("licence.issue")?;

    let detail = licensees::get_licensee(&state.pool, user.scope(), id)
        .await
        .map_err(database_error)?
        .ok_or(AppError::NotFound { entity: "licence profile", label: "That licence profile".into() })?;

    Ok(Json(detail))
}

#[derive(Deserialize)]
struct PersonInput {
    full_name: String,
    username: String,
    #[serde(default)]
    position_title: Option<String>,
}

/// How long a key gives access for.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Access {
    /// One month; renewed by issuing a new key each month.
    Monthly,
    /// Twelve months.
    Annual,
    /// Never expires.
    Lifetime,
}

impl Access {
    /// Recorded as the key's plan, so the office sees it on System → Subscription.
    fn plan(self) -> &'static str {
        match self {
            Access::Monthly => "monthly",
            Access::Annual => "annual",
            Access::Lifetime => "lifetime",
        }
    }
}

/// The end of a lifetime key. The token format has no "never", so lifetime is
/// the last day the system can show, and every screen reads it as "Lifetime".
fn lifetime_end() -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(9999, 12, 31)
        .expect("a valid date")
        .and_hms_opt(0, 0, 0)
        .expect("midnight exists")
        .and_utc()
}

/// When a key starting at `start` with this access ends.
///
/// Calendar months, so a twelve-month key from 1 January ends on 1 January,
/// not on the 27th of December. A month from 31 January ends on the last day
/// of February.
fn expiry(access: Access, start: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match access {
        Access::Monthly => start.checked_add_months(Months::new(1)),
        Access::Annual => start.checked_add_months(Months::new(12)),
        Access::Lifetime => Some(lifetime_end()),
    }
}

/// How a key gives access, and for how many workstations.
#[derive(Deserialize)]
struct TermsInput {
    access: Access,
    seats: u32,
    /// Defaults to today. A renewal bought early starts when the old key
    /// ends, so the office does not forfeit the time it already paid for.
    #[serde(default)]
    starts_on: Option<NaiveDate>,
}

/// Step one of issuing: who the key is for, and on what terms.
#[derive(Deserialize)]
struct DetailsInput {
    municipality_name: String,
    /// Bind the key to the municipality this very server serves, so it can be
    /// installed here. Otherwise a new municipality is minted for it.
    for_this_server: bool,
    accountant: PersonInput,
    #[serde(default)]
    staff: Vec<PersonInput>,
    terms: TermsInput,
}

/// A generated key, not yet saved to any profile.
#[derive(Serialize)]
struct GeneratedKey {
    token: String,
    municipality_name: String,
    for_this_server: bool,
    target_tenant_code: String,
    access: Access,
    seats: u32,
    not_before: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    grace_days: i64,
    people: Vec<NewPerson>,
}

/// Step two: signs the key and shows it. Saves nothing.
///
/// A signed key is valid the moment it exists, saved or not, so generating
/// one is written to the audit trail even though no profile is made yet.
async fn generate_licence(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(input): JsonBody<DetailsInput>,
) -> ApiResult<Json<GeneratedKey>> {
    user.require("licence.issue")?;
    let issuer = issuer(&state)?;

    let mut fields = BTreeMap::new();

    let municipality = input.municipality_name.trim().to_string();
    if municipality.is_empty() {
        fields.insert("municipality_name".into(), "Choose the municipality.".into());
    } else if municipality.chars().count() > 120 {
        fields.insert("municipality_name".into(), "That name is too long.".into());
    }

    let people = collect_people(&mut fields, &input.accountant, &input.staff);
    check_terms(&mut fields, &input.terms);

    if !fields.is_empty() {
        return Err(AppError::Validation { fields });
    }

    let (target_tenant_id, target_tenant_code) = if input.for_this_server {
        let (code, _) = licensees::tenant_identity(&state.pool, state.node.tenant_id)
            .await
            .map_err(database_error)?;
        (state.node.tenant_id, code)
    } else {
        (Uuid::now_v7(), tenant_code_from_name(&municipality))
    };

    // Refused now rather than at step three, after the key has been copied
    // and perhaps already sent.
    if licensees::find_licensee(&state.pool, user.scope(), &target_tenant_code, target_tenant_id)
        .await
        .map_err(database_error)?
        .is_some()
    {
        return Err(AppError::Rule {
            code: "LICENCE_PROFILE_EXISTS",
            message: format!("{municipality} already has a licence profile."),
            hint: Some("Open it from the list of licences and issue a new key from there.".into()),
        });
    }

    let key = sign(&issuer, target_tenant_id, &municipality, &people, &input.terms)?;

    licensees::record_generation(
        &state.pool,
        user.scope(),
        &user.username,
        &municipality,
        &key.plan,
        key.expires_at,
    )
    .await
    .map_err(database_error)?;

    Ok(Json(GeneratedKey {
        token: key.token,
        municipality_name: municipality,
        for_this_server: input.for_this_server,
        target_tenant_code,
        access: input.terms.access,
        seats: input.terms.seats,
        not_before: key.not_before,
        expires_at: key.expires_at,
        grace_days: GRACE_DAYS,
        people,
    }))
}

#[derive(Deserialize)]
struct SaveInput {
    token: String,
    #[serde(default)]
    notes: Option<String>,
}

/// Step three: saves the profile, from the key itself.
///
/// The profile is read out of the signed key rather than taken from the form,
/// so what is saved is exactly what was generated and copied — a key edited
/// in between fails its signature and is refused.
async fn save_licensee(
    State(state): State<AppState>,
    user: CurrentUser,
    JsonBody(input): JsonBody<SaveInput>,
) -> ApiResult<Json<LicenseeDetail>> {
    user.require("licence.issue")?;

    let public_key = token::authority_public_key()
        .map_err(|e| AppError::Unexpected(e.context("this installation cannot check licences")))?;

    let token_text = input.token.trim().to_string();
    let claims = token::verify(&token_text, &public_key).map_err(|e| AppError::Rule {
        code: "LICENCE_NOT_GENUINE",
        message: as_sentence(&e.to_string()),
        hint: Some("Generate the licence again, then save it without changing it.".into()),
    })?;

    if claims.people.is_empty() {
        return Err(AppError::Rule {
            code: "LICENCE_HAS_NO_PEOPLE",
            message: "That key does not name the municipality's accountant and staff, so it cannot become a profile.".into(),
            hint: Some("Generate the licence here, then save it.".into()),
        });
    }

    let for_this_server = claims.tenant_id == state.node.tenant_id;
    let target_tenant_code = if for_this_server {
        licensees::tenant_identity(&state.pool, state.node.tenant_id)
            .await
            .map_err(database_error)?
            .0
    } else {
        tenant_code_from_name(&claims.municipality)
    };

    let people = claims
        .people
        .iter()
        .map(|p| NewPerson {
            full_name: p.full_name.clone(),
            username: p.username.clone(),
            role: p.role.clone(),
            position_title: p.position_title.clone(),
        })
        .collect();

    let record = IssueRecord {
        token: token_text,
        plan: claims.plan.clone(),
        seats: i32::try_from(claims.seats).unwrap_or(i32::MAX),
        not_before: claims.not_before,
        expires_at: claims.expires_at,
        grace_days: i32::try_from(claims.grace_days).unwrap_or(0),
    };

    let detail = licensees::create_licensee(
        &state.pool,
        user.scope(),
        &user.username,
        NewLicensee {
            municipality_name: claims.municipality.clone(),
            target_tenant_id: claims.tenant_id,
            target_tenant_code,
            for_this_server,
            notes: input.notes,
            people,
        },
        record,
    )
    .await
    .map_err(database_error)?;

    tracing::info!(profile = %detail.id, "a licence profile was saved");
    Ok(Json(detail))
}

#[derive(Deserialize)]
struct RenewInput {
    #[serde(default)]
    notes: Option<String>,
}

/// Records a paid renewal: another month or year on the same key.
///
/// No key is issued and nothing is sent to the municipality. Its server
/// collects the new end date the next time it checks in, and the office can
/// make it check at once with "Check for renewal".
async fn renew(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    JsonBody(input): JsonBody<RenewInput>,
) -> ApiResult<Json<LicenseeDetail>> {
    user.require("licence.issue")?;

    let detail = licensees::renew_licensee(&state.pool, user.scope(), &user.username, id, input.notes)
        .await
        .map_err(database_error)?
        .ok_or(AppError::NotFound { entity: "licence profile", label: "That licence profile".into() })?;

    tracing::info!(
        profile = %detail.id,
        paid_through = ?detail.paid_through,
        "a subscription was renewed"
    );
    Ok(Json(detail))
}

fn issuer(state: &AppState) -> Result<std::sync::Arc<token::AuthorityKey>, AppError> {
    state.issuer.clone().ok_or_else(|| AppError::Rule {
        code: "ISSUING_NOT_AVAILABLE",
        message: "This server cannot issue licence keys.".into(),
        hint: Some(
            "Keys are issued from the licensing authority's own server, which holds the \
             signing key. A municipality's server never has it."
                .into(),
        ),
    })
}

/// Signs a key, returning it as the record that will be kept of it.
fn sign(
    issuer: &token::AuthorityKey,
    tenant_id: Uuid,
    municipality: &str,
    people: &[NewPerson],
    terms: &TermsInput,
) -> Result<IssueRecord, AppError> {
    let now = Utc::now();

    let not_before = match terms.starts_on {
        Some(day) => day.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc(),
        None => now,
    };

    let expires_at = expiry(terms.access, not_before).ok_or_else(|| {
        AppError::validation("terms.starts_on", "That start date is past any date the system can record.")
    })?;

    let claims = Claims {
        tenant_id,
        municipality: municipality.to_string(),
        plan: terms.access.plan().to_string(),
        seats: terms.seats,
        issued_at: now,
        not_before,
        expires_at,
        grace_days: GRACE_DAYS,
        fingerprint: None,
        features: vec![],
        people: people
            .iter()
            .map(|p| LicensedPerson {
                username: p.username.clone(),
                full_name: p.full_name.trim().to_string(),
                role: p.role.clone(),
                position_title: p.position_title.clone(),
            })
            .collect(),
    };

    Ok(IssueRecord {
        token: token::issue(&claims, issuer).map_err(AppError::Unexpected)?,
        plan: claims.plan,
        seats: i32::try_from(claims.seats).unwrap_or(i32::MAX),
        not_before,
        expires_at,
        grace_days: GRACE_DAYS as i32,
    })
}

// ── Checking what was entered ────────────────────────────────────────────────

fn normalise_username(raw: &str) -> String {
    raw.trim().to_lowercase()
}

fn check_username(fields: &mut BTreeMap<String, String>, key: &str, username: &str) {
    let ok = (3..=40).contains(&username.chars().count())
        && username
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));

    if !ok {
        fields.insert(
            key.into(),
            "A username is 3 to 40 letters, digits, dots, dashes, or underscores — like m.santos."
                .into(),
        );
    }
}

/// Turns the accountant and staff rows into people, checking each and that no
/// username is used twice on the same profile.
fn collect_people(
    fields: &mut BTreeMap<String, String>,
    accountant: &PersonInput,
    staff: &[PersonInput],
) -> Vec<NewPerson> {
    let mut people = Vec::with_capacity(1 + staff.len());

    let rows = std::iter::once(("accountant".to_string(), accountant, "ACCOUNTANT"))
        .chain(staff.iter().enumerate().map(|(i, p)| (format!("staff.{i}"), p, "STAFF")));

    for (prefix, person, role) in rows {
        let username = normalise_username(&person.username);

        if person.full_name.trim().is_empty() {
            fields.insert(format!("{prefix}.full_name"), "Enter the person's full name.".into());
        }
        check_username(fields, &format!("{prefix}.username"), &username);

        if people.iter().any(|p: &NewPerson| p.username == username) {
            fields.insert(
                format!("{prefix}.username"),
                "This username is already used by someone else on this profile.".into(),
            );
        }

        // Titled as the JEV titles its signatories unless the office says
        // otherwise: "Municipal Accountant" and "Accounting Staff".
        let default_title = if role == "ACCOUNTANT" { "Municipal Accountant" } else { "Accounting Staff" };

        people.push(NewPerson {
            full_name: person.full_name.trim().to_string(),
            username,
            role: role.to_string(),
            position_title: person
                .position_title
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .unwrap_or(default_title)
                .to_string()
                .into(),
        });
    }

    if staff.is_empty() {
        fields.insert("staff".into(), "Add at least one member of the Accounting Staff.".into());
    }

    people
}

fn check_terms(fields: &mut BTreeMap<String, String>, terms: &TermsInput) {
    // The access type needs no check here: anything but monthly, annual, or
    // lifetime is refused when the request is read.
    if !(1..=500).contains(&terms.seats) {
        fields.insert("terms.seats".into(), "Workstations must be between 1 and 500.".into());
    }
}

/// A short code for a municipality, from its name: `Municipality of Boac` →
/// `BOAC`. It names the municipality in the server's settings, so it only has
/// to be readable and unique among the profiles here.
fn tenant_code_from_name(name: &str) -> String {
    let lowered = name.trim().to_lowercase();
    let stripped = ["municipality of ", "city of ", "province of "]
        .iter()
        .find_map(|prefix| lowered.strip_prefix(prefix))
        .unwrap_or(&lowered);

    let mut code = String::new();
    for c in stripped.chars() {
        if c.is_ascii_alphanumeric() {
            code.push(c.to_ascii_uppercase());
        } else if !code.ends_with('-') && !code.is_empty() {
            code.push('-');
        }
    }

    let code: String = code.chars().take(40).collect();
    let code = code.trim_end_matches('-').to_string();

    if code.len() >= 2 {
        code
    } else {
        // Nothing usable in the name: still a valid, unique code.
        let tail = Uuid::now_v7().simple().to_string().to_uppercase();
        format!("LGU-{}", &tail[tail.len() - 8..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_municipality_code_reads_like_the_municipality() {
        assert_eq!(tenant_code_from_name("Municipality of Boac"), "BOAC");
        assert_eq!(tenant_code_from_name("  municipality of Santa Cruz "), "SANTA-CRUZ");
        assert_eq!(tenant_code_from_name("City of Calapan"), "CALAPAN");
        assert_eq!(tenant_code_from_name("Torrijos"), "TORRIJOS");
    }

    #[test]
    fn a_name_with_nothing_usable_still_gets_a_valid_code() {
        let code = tenant_code_from_name("—");
        assert!(code.starts_with("LGU-") && code.len() >= 6, "{code}");
    }

    #[test]
    fn each_access_type_ends_when_the_office_would_expect() {
        let jan_1 = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc();
        let jan_31 = NaiveDate::from_ymd_opt(2027, 1, 31).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc();

        assert_eq!(expiry(Access::Monthly, jan_1).unwrap().date_naive().to_string(), "2027-02-01");
        // A month from 31 January is the end of February, not 3 March.
        assert_eq!(expiry(Access::Monthly, jan_31).unwrap().date_naive().to_string(), "2027-02-28");
        assert_eq!(expiry(Access::Annual, jan_1).unwrap().date_naive().to_string(), "2028-01-01");
        assert_eq!(expiry(Access::Lifetime, jan_1).unwrap().date_naive().to_string(), "9999-12-31");
    }

    #[test]
    fn access_is_read_only_as_one_of_the_three_offered() {
        for (text, access) in [("monthly", Access::Monthly), ("annual", Access::Annual), ("lifetime", Access::Lifetime)] {
            let parsed: Access = serde_json::from_value(serde_json::json!(text)).unwrap();
            assert_eq!(parsed, access);
            assert_eq!(parsed.plan(), text);
        }
        assert!(serde_json::from_value::<Access>(serde_json::json!("forever")).is_err());
    }

    #[test]
    fn usernames_are_checked_the_way_the_form_explains_them() {
        let mut f = BTreeMap::new();
        for good in ["m.santos", "jreyes", "staff_01", "ana-lim"] {
            check_username(&mut f, "u", good);
        }
        assert!(f.is_empty(), "{f:?}");

        for bad in ["ab", "Maria Santos", "m@santos", ""] {
            let mut f = BTreeMap::new();
            check_username(&mut f, "u", bad);
            assert!(f.contains_key("u"), "{bad:?} was accepted");
        }
    }

    #[test]
    fn a_profile_cannot_give_two_people_the_same_username() {
        let mut f = BTreeMap::new();
        let accountant = PersonInput {
            full_name: "Maria Santos".into(),
            username: "m.santos".into(),
            position_title: None,
        };
        let staff = vec![PersonInput {
            full_name: "Mario Santos".into(),
            username: "M.Santos".into(),
            position_title: None,
        }];

        let people = collect_people(&mut f, &accountant, &staff);
        assert_eq!(people.len(), 2);
        assert!(f.contains_key("staff.0.username"), "{f:?}");
    }

    #[test]
    fn the_accountant_is_titled_as_the_form_titles_them() {
        let mut f = BTreeMap::new();
        let accountant = PersonInput {
            full_name: "Maria Santos".into(),
            username: "m.santos".into(),
            position_title: None,
        };
        let staff = vec![PersonInput {
            full_name: "Juan Reyes".into(),
            username: "j.reyes".into(),
            position_title: None,
        }];

        let people = collect_people(&mut f, &accountant, &staff);
        assert!(f.is_empty(), "{f:?}");
        assert_eq!(people[0].role, "ACCOUNTANT");
        assert_eq!(people[0].position_title.as_deref(), Some("Municipal Accountant"));
        assert_eq!(people[1].role, "STAFF");
        assert_eq!(people[1].position_title.as_deref(), Some("Accounting Staff"));
    }
}
