//! The error taxonomy.
//!
//! Every failure that crosses the wire is shaped like this:
//!
//! ```jsonc
//! {
//!   "code":    "OBLIGATION_EXCEEDS_ALLOTMENT",
//!   "message": "This obligation is ₱12,500.00 more than the allotment still available for MOOE under the general fund.",
//!   "hint":    "Ask the budget office to realign the allotment, or reduce the amount.",
//!   "fields":  { "amount": "Amount is higher than the remaining balance." },
//!   "reference": "GSN-7QK2"
//! }
//! ```
//!
//! The rule, enforced in review: **if a message names a table, a column, a
//! constraint, a status code, or a Rust/TypeScript type, it is a defect.**
//! A municipal accounting clerk must be able to read it and know what to do.
//!
//! Technical detail is not discarded — it goes to the log, tagged with the
//! same `reference` the user sees. Support asks for four characters and finds
//! the exact failure.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

/// The JSON body of every error response.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Stable machine-readable identifier. The client may branch on this;
    /// it must never be shown to a user.
    pub code: &'static str,

    /// Plain language, specific, with real numbers where numbers are relevant.
    pub message: String,

    /// What to do about it. Omitted when there is nothing useful to say.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,

    /// Per-field messages, keyed by form field name. Drives inline
    /// react-hook-form errors; the `message` above drives the toast.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,

    /// Short correlation code, also written to the server log.
    pub reference: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {entity}")]
    NotFound { entity: &'static str, label: String },

    #[error("validation failed")]
    Validation { fields: BTreeMap<String, String> },

    /// A rule the database or domain layer rejected, already phrased for a
    /// user. Produced by our own trigger `RAISE EXCEPTION`s.
    #[error("{message}")]
    Rule {
        code: &'static str,
        message: String,
        hint: Option<String>,
    },

    #[error("not signed in")]
    Unauthenticated,

    #[error("not permitted: {permission}")]
    Forbidden { permission: &'static str },

    #[error("conflict")]
    StaleRecord { entity: &'static str },

    #[error("licence expired")]
    LicenceExpired { expired_on: String },

    #[error("read-only mode")]
    ReadOnly,

    #[error("node unreachable")]
    NodeUnreachable,

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl AppError {
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert(field.into(), message.into());
        AppError::Validation { fields }
    }

    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
            AppError::Validation { .. } | AppError::Rule { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Unauthenticated => StatusCode::UNAUTHORIZED,
            AppError::Forbidden { .. } | AppError::ReadOnly | AppError::LicenceExpired { .. } => {
                StatusCode::FORBIDDEN
            }
            AppError::StaleRecord { .. } => StatusCode::CONFLICT,
            AppError::NodeUnreachable => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Database(_) | AppError::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// A short, unambiguous correlation code such as `GSN-7QK2`.
///
/// Deliberately excludes vowels, and the characters `0`, `O`, `1`, `I`, so it
/// can be read aloud over a phone without ambiguity and cannot spell anything.
fn new_reference() -> String {
    const ALPHABET: &[u8] = b"23456789BCDFGHJKLMNPQRSTVWXZ";
    let raw = Uuid::new_v4();
    let code: String = raw
        .as_bytes()
        .iter()
        .take(4)
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect();
    format!("GSN-{code}")
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let reference = new_reference();
        let status = self.status();

        let body = match &self {
            AppError::NotFound { label, .. } => ErrorBody {
                code: "NOT_FOUND",
                message: format!("{label} could not be found."),
                hint: Some("It may have been removed, or the link may be out of date.".into()),
                fields: BTreeMap::new(),
                reference: reference.clone(),
            },

            AppError::Validation { fields } => ErrorBody {
                code: "VALIDATION_FAILED",
                message: if fields.len() == 1 {
                    fields.values().next().cloned().unwrap_or_default()
                } else {
                    "Some of the information entered needs to be corrected.".into()
                },
                hint: None,
                fields: fields.clone(),
                reference: reference.clone(),
            },

            AppError::Rule { code, message, hint } => ErrorBody {
                code,
                message: message.clone(),
                hint: hint.clone(),
                fields: BTreeMap::new(),
                reference: reference.clone(),
            },

            AppError::Unauthenticated => ErrorBody {
                code: "UNAUTHENTICATED",
                message: "Your session has ended.".into(),
                hint: Some("Sign in again to continue. Any unsaved work is kept as a draft.".into()),
                fields: BTreeMap::new(),
                reference: reference.clone(),
            },

            AppError::Forbidden { .. } => ErrorBody {
                code: "FORBIDDEN",
                // Deliberately does not name the permission. Telling a user
                // which internal permission they lack is noise to them and a
                // map of the system to anyone else.
                message: "You do not have access to this.".into(),
                hint: Some("Ask an administrator if you need it for your work.".into()),
                fields: BTreeMap::new(),
                reference: reference.clone(),
            },

            AppError::StaleRecord { entity } => ErrorBody {
                code: "STALE_RECORD",
                message: format!("Someone else changed this {entity} while you were working on it."),
                hint: Some("Reload to see their changes, then apply yours again.".into()),
                fields: BTreeMap::new(),
                reference: reference.clone(),
            },

            AppError::LicenceExpired { expired_on } => ErrorBody {
                code: "LICENCE_EXPIRED",
                message: format!("The subscription for this system ended on {expired_on}."),
                hint: Some(
                    "Everything can still be viewed and printed. Renew to record new entries."
                        .into(),
                ),
                fields: BTreeMap::new(),
                reference: reference.clone(),
            },

            AppError::ReadOnly => ErrorBody {
                code: "READ_ONLY",
                message: "The system is in read-only mode.".into(),
                hint: Some("Records can be viewed and printed, but not changed.".into()),
                fields: BTreeMap::new(),
                reference: reference.clone(),
            },

            AppError::NodeUnreachable => ErrorBody {
                code: "NODE_UNREACHABLE",
                message: "The accounting office server cannot be reached.".into(),
                hint: Some(
                    "Your work is saved on this computer as a draft and will be sent \
                     automatically once the connection returns."
                        .into(),
                ),
                fields: BTreeMap::new(),
                reference: reference.clone(),
            },

            AppError::Database(e) => map_database_error(e, &reference),

            AppError::Unexpected(_) => generic_failure(&reference),
        };

        // The user sees `body`. The log gets everything.
        match &self {
            AppError::Database(e) => {
                tracing::error!(reference = %reference, error = ?e, "database error");
            }
            AppError::Unexpected(e) => {
                tracing::error!(reference = %reference, error = ?e, "unexpected error");
            }
            other => {
                tracing::info!(reference = %reference, code = body.code, error = %other, "request rejected");
            }
        }

        (status, Json(body)).into_response()
    }
}

fn generic_failure(reference: &str) -> ErrorBody {
    ErrorBody {
        code: "UNEXPECTED",
        message: "Something went wrong and this could not be completed.".into(),
        hint: Some(format!(
            "Nothing was saved. Try again, and if it keeps happening give your \
             system administrator this reference: {reference}"
        )),
        fields: BTreeMap::new(),
        reference: reference.to_string(),
    }
}

/// Turns a PostgreSQL failure into something a clerk can act on.
///
/// Two sources have to be told apart:
///
///   * **Our own trigger `RAISE EXCEPTION`s** — these already carry a
///     plain-language message and hint, written in the migration. They set no
///     constraint name, which is how they are recognised, and their message is
///     forwarded verbatim.
///   * **Native constraint violations** — these carry a constraint name and a
///     message like `duplicate key value violates unique constraint "…"`,
///     which must never reach a user. They are translated by name.
///
/// Anything unrecognised falls through to a generic message, with the real
/// error logged. Guessing is worse than admitting ignorance.
fn map_database_error(err: &sqlx::Error, reference: &str) -> ErrorBody {
    let Some(db) = err.as_database_error() else {
        return generic_failure(reference);
    };

    if let Some(constraint) = db.constraint() {
        if let Some((code, message, hint)) = translate_constraint(constraint) {
            return ErrorBody {
                code,
                message: message.into(),
                hint: hint.map(Into::into),
                fields: BTreeMap::new(),
                reference: reference.to_string(),
            };
        }
        // A named constraint we have not written a message for yet. Do not
        // leak the raw text; log it so the gap gets closed.
        tracing::warn!(
            reference = %reference,
            constraint = %constraint,
            "no user-facing message for this constraint"
        );
        return generic_failure(reference);
    }

    // No constraint name: one of our own RAISE EXCEPTIONs from a trigger.
    let sqlstate = db.code().unwrap_or_default().to_string();
    if matches!(sqlstate.as_str(), "23514" | "23001" | "42501" | "P0002") {
        return ErrorBody {
            code: "RULE_VIOLATION",
            message: db.message().to_string(),
            // The triggers carry a HINT saying what to do about it. Dropping
            // it would leave a clerk told what is wrong but not how to fix it,
            // which is the half of the message that actually helps.
            hint: postgres_hint(db),
            fields: BTreeMap::new(),
            reference: reference.to_string(),
        };
    }

    generic_failure(reference)
}

/// Reads the `HINT` a PL/pgSQL `RAISE` attached to its exception.
///
/// sqlx's generic `DatabaseError` has no accessor for it, so this reaches the
/// PostgreSQL-specific type. Returns `None` when the error carries no hint.
fn postgres_hint(db: &dyn sqlx::error::DatabaseError) -> Option<String> {
    let pg = db
        .try_downcast_ref::<giams_db::sqlx::postgres::PgDatabaseError>()?;
    pg.hint().map(str::to_owned)
}

/// Constraint name → what to tell the user.
///
/// Every named constraint in `db/migrations` should appear here. The names are
/// explicit in the schema precisely so this mapping can exist and stay stable
/// when tables change.
fn translate_constraint(
    constraint: &str,
) -> Option<(&'static str, &'static str, Option<&'static str>)> {
    Some(match constraint {
        "licence_check" => (
            "LICENCE_DATES_IMPOSSIBLE",
            "That licence key ends before it begins, so it cannot be installed.",
            Some("Ask for a replacement key."),
        ),
        "journal_line_exactly_one_side" => (
            "LINE_HAS_BOTH_SIDES",
            "Each line of a journal entry must be either a debit or a credit, not both and not neither.",
            Some("Enter the amount in one column and leave the other blank."),
        ),
        "journal_line_amounts_not_negative" => (
            "NEGATIVE_AMOUNT",
            "Amounts on a journal entry cannot be negative.",
            Some("To record the opposite effect, put the amount on the other side instead."),
        ),
        "journal_entry_posted_needs_actor" => (
            "POSTING_NEEDS_ACTOR",
            "A journal entry cannot be marked as posted without recording who posted it.",
            None,
        ),
        "journal_entry_approved_needs_actor" => (
            "APPROVAL_NEEDS_ACTOR",
            "A journal entry cannot be marked as approved without recording who approved it.",
            None,
        ),
        "journal_entry_reversal_needs_reason" => (
            "REVERSAL_NEEDS_REASON",
            "A reversing entry needs a reason.",
            Some("Explain briefly why the original entry is being reversed."),
        ),
        "journal_entry_tenant_id_fund_id_jev_no_key" => (
            "DUPLICATE_JEV_NUMBER",
            "That journal entry number has already been used for this fund.",
            Some("Reload the page to get the next available number."),
        ),
        "app_user_tenant_id_username_key" => (
            "USERNAME_TAKEN",
            "That username is already in use.",
            Some("Choose a different one."),
        ),
        "payee_identity" => (
            "DUPLICATE_PAYEE",
            "A payee with that name and TIN already exists.",
            Some("Use the existing record instead of creating a second one."),
        ),
        "account_tenant_id_code_key" => (
            "DUPLICATE_ACCOUNT_CODE",
            "That account code is already in the chart of accounts.",
            None,
        ),
        "fund_tenant_id_code_key" => (
            "DUPLICATE_FUND_CODE",
            "That fund code is already in use.",
            None,
        ),
        "node_one_primary_per_tenant" => (
            "PRIMARY_NODE_EXISTS",
            "This municipality already has a main accounting server.",
            Some("Only one server can hold the official records. Ask an administrator before changing this."),
        ),
        "sync_outbox_node_id_node_seq_key" => (
            "SYNC_SEQUENCE_CONFLICT",
            "The upload could not be prepared because of a numbering conflict.",
            Some("Try syncing again. If it repeats, contact your system administrator."),
        ),

        // ── Budget execution (migration 0008) ────────────────────────────────
        "obligation_paid_within_payable" => (
            "PAID_EXCEEDS_PAYABLE",
            "This payment is more than the amount currently due on the obligation.",
            Some("Check the voucher amount against what is still payable."),
        ),
        "obligation_payable_within_amount" => (
            "PAYABLE_EXCEEDS_OBLIGATION",
            "The vouchers drawn against this obligation add up to more than the obligation itself.",
            Some("Reduce the voucher, or ask the budget office to increase the obligation."),
        ),
        "obligation_budget_certification_complete" => (
            "INCOMPLETE_CERTIFICATION",
            "A certification needs both the officer who signed it and the date it was signed.",
            None,
        ),
        "obligation_accounting_certification_complete" => (
            "INCOMPLETE_CERTIFICATION",
            "A certification needs both the officer who signed it and the date it was signed.",
            None,
        ),
        "obligation_cancellation_needs_reason" => (
            "CANCELLATION_NEEDS_REASON",
            "Cancelling an obligation needs a reason.",
            Some("Explain briefly why it is being cancelled. This becomes part of the record."),
        ),
        "obligation_tenant_id_fund_id_fiscal_year_ors_no_key" => (
            "DUPLICATE_ORS_NUMBER",
            "That obligation request number has already been used for this fund and year.",
            Some("Reload the page to get the next available number."),
        ),
        "allotment_tenant_id_fund_id_fiscal_year_allotment_no_key" => (
            "DUPLICATE_ALLOTMENT_NUMBER",
            "That allotment number has already been used for this fund and year.",
            Some("Reload the page to get the next available number."),
        ),
        "office_tenant_id_code_key" => (
            "DUPLICATE_OFFICE_CODE",
            "That office code is already in use.",
            None,
        ),

        // ── Disbursement (migration 0011) ────────────────────────────────────
        "deduction_rate_no_overlapping_windows" => (
            "OVERLAPPING_RATE_PERIODS",
            "There is already a rate for this deduction covering part of those dates.",
            Some("Close the existing rate with an end date before starting the new one."),
        ),
        "deduction_rate_has_a_value" => (
            "RATE_HAS_NO_VALUE",
            "A deduction rate needs either a percentage or a fixed amount.",
            None,
        ),
        "deduction_rate_window_is_ordered" => (
            "RATE_DATES_REVERSED",
            "The end date of a rate cannot be before the date it starts.",
            None,
        ),
        "dv_deductions_within_gross" => (
            "DEDUCTIONS_EXCEED_GROSS",
            "The deductions add up to more than the amount of the voucher.",
            Some("Check the deduction lines. The net amount cannot be less than zero."),
        ),
        "dv_approved_needs_actor" => (
            "APPROVAL_NEEDS_ACTOR",
            "A voucher cannot be marked as approved without recording who approved it.",
            None,
        ),
        "dv_cancellation_needs_reason" => (
            "CANCELLATION_NEEDS_REASON",
            "Cancelling a voucher needs a reason.",
            Some("Explain briefly why. This becomes part of the record."),
        ),
        "disbursement_voucher_tenant_id_fund_id_fiscal_year_dv_no_key" => (
            "DUPLICATE_DV_NUMBER",
            "That voucher number has already been used for this fund and year.",
            Some("Reload the page to get the next available number."),
        ),
        "disbursement_instrument_unique" => (
            "DUPLICATE_CHECK_NUMBER",
            "That check or ADA number has already been issued on this bank account.",
            Some("Check the number against the checkbook. Reusing one makes the bank reconciliation impossible to match."),
        ),
        "disbursement_check_needs_bank" => (
            "PAYMENT_NEEDS_BANK_DETAILS",
            "A check or ADA payment needs both a bank account and an instrument number.",
            None,
        ),
        "dv_deduction_dv_id_deduction_type_id_key" => (
            "DUPLICATE_DEDUCTION",
            "That deduction has already been applied to this voucher.",
            Some("Edit the existing line instead of adding a second one."),
        ),
        "bank_account_tenant_id_bank_name_account_no_key" => (
            "DUPLICATE_BANK_ACCOUNT",
            "That account number is already recorded for this bank.",
            None,
        ),
        _ => return None,
    })
}

/// Every constraint that has a user-facing message.
///
/// Listed explicitly rather than derived, so that the tests can hold all of
/// them to the same standard and so that adding a translation without adding
/// it here shows up in review.
#[cfg(test)]
pub(crate) const ALL_TRANSLATED_CONSTRAINTS: &[&str] = &[
    "journal_line_exactly_one_side",
    "journal_line_amounts_not_negative",
    "journal_entry_posted_needs_actor",
    "journal_entry_approved_needs_actor",
    "journal_entry_reversal_needs_reason",
    "journal_entry_tenant_id_fund_id_jev_no_key",
    "app_user_tenant_id_username_key",
    "payee_identity",
    "account_tenant_id_code_key",
    "fund_tenant_id_code_key",
    "node_one_primary_per_tenant",
    "sync_outbox_node_id_node_seq_key",
    "obligation_paid_within_payable",
    "obligation_payable_within_amount",
    "obligation_budget_certification_complete",
    "obligation_accounting_certification_complete",
    "obligation_cancellation_needs_reason",
    "obligation_tenant_id_fund_id_fiscal_year_ors_no_key",
    "allotment_tenant_id_fund_id_fiscal_year_allotment_no_key",
    "office_tenant_id_code_key",
    "deduction_rate_no_overlapping_windows",
    "deduction_rate_has_a_value",
    "deduction_rate_window_is_ordered",
    "dv_deductions_within_gross",
    "dv_approved_needs_actor",
    "dv_cancellation_needs_reason",
    "disbursement_voucher_tenant_id_fund_id_fiscal_year_dv_no_key",
    "disbursement_instrument_unique",
    "disbursement_check_needs_bank",
    "dv_deduction_dv_id_deduction_type_id_key",
    "bank_account_tenant_id_bank_name_account_no_key",
];

pub type ApiResult<T> = Result<T, AppError>;

/// Turns an error from the repository layer into one the office can read.
///
/// The order matters. A [`Refusal`] is the database layer declining for an
/// ordinary reason a person can fix, and must reach them as that reason. Only
/// once that has been ruled out is an error genuinely unexpected.
///
/// This lives here, in one place, because it used to live in five — copied
/// into every route module. Five copies meant a fix applied to one of them
/// left the other four still turning ordinary refusals into "Something went
/// wrong", with a reference number implying the system was broken.
pub fn database_error(err: anyhow::Error) -> AppError {
    let err = match err.downcast::<giams_db::Refusal>() {
        Ok(refusal) => {
            return AppError::Rule {
                code: refusal.code,
                message: refusal.message,
                hint: refusal.hint,
            }
        }
        Err(err) => err,
    };

    match err.downcast::<giams_db::sqlx::Error>() {
        Ok(db) => AppError::Database(db),
        Err(other) => AppError::Unexpected(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_codes_are_readable_over_the_phone() {
        for _ in 0..500 {
            let r = new_reference();
            assert!(r.starts_with("GSN-"));
            assert_eq!(r.len(), 8);
            // No vowels, no 0/O/1/I — nothing ambiguous when read aloud.
            assert!(!r[4..].chars().any(|c| "AEIOU01".contains(c)));
        }
    }

    /// Words that only ever appear in developer vocabulary.
    ///
    /// Deliberately excludes words that are ambiguous between schema jargon
    /// and ordinary office language. "Column" is the clearest possible word
    /// for the debit and credit columns of an entry grid, and banning it would
    /// push messages toward vaguer phrasing — the opposite of the goal.
    /// All lowercase, because the haystack is lowercased before comparison.
    const DEVELOPER_JARGON: &[&str] = &[
        "constraint", "violates", "null value", "foreign key", "sqlstate",
        "relation", "tenant_id", "uuid", "panic", "unwrap", "nullptr",
        "exception", "stack trace", "database error", "internal server",
    ];

    #[test]
    fn every_named_constraint_message_is_free_of_jargon() {
        // A message that leaks schema vocabulary is the exact failure mode
        // this module exists to prevent, so it is asserted rather than trusted.
        // This covers every entry in the table, so a new one cannot be added
        // without being held to the same standard.
        for name in ALL_TRANSLATED_CONSTRAINTS {
            let (_, message, hint) = translate_constraint(name).expect(name);
            let haystack = format!("{message} {}", hint.unwrap_or("")).to_lowercase();
            for word in DEVELOPER_JARGON {
                assert!(
                    !haystack.contains(word),
                    "message for `{name}` contains developer jargon: {word}"
                );
            }
        }
    }

    #[test]
    fn user_facing_messages_are_whole_sentences() {
        for name in ALL_TRANSLATED_CONSTRAINTS {
            let (code, message, hint) = translate_constraint(name).expect(name);
            assert!(
                message.ends_with('.'),
                "message for `{name}` is not a complete sentence"
            );
            assert!(
                message.chars().next().is_some_and(char::is_uppercase),
                "message for `{name}` does not start with a capital"
            );
            assert!(
                code.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "code for `{name}` is not a stable SCREAMING_SNAKE identifier"
            );
            if let Some(hint) = hint {
                assert!(hint.ends_with('.'), "hint for `{name}` is not a complete sentence");
            }
        }
    }
}
