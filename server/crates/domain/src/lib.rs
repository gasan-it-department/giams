//! GIAMS domain layer — pure accounting logic.
//!
//! Nothing in this crate touches a database, a socket, or a clock it did not
//! receive as an argument. That constraint is deliberate: the arithmetic that
//! decides how public money is recorded should be testable exhaustively and
//! without infrastructure.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod money;

pub use money::Money;

use serde::{Deserialize, Serialize};

/// The books of original entry maintained under the GAM for LGUs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JournalType {
    /// General Journal — adjusting, closing, and non-cash entries.
    Gj,
    /// Cash Receipts Journal — collections.
    Crj,
    /// Cash Disbursements Journal — cash and ADA payments.
    Cdj,
    /// Check Disbursements Journal.
    CkDj,
}

/// The five account groups of the Revised Chart of Accounts, identified by the
/// leading digit of the 8-digit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    Asset,
    Liability,
    Equity,
    Income,
    Expense,
}

impl AccountType {
    /// The side on which this kind of account increases.
    pub fn normal_balance(self) -> Side {
        match self {
            AccountType::Asset | AccountType::Expense => Side::Debit,
            AccountType::Liability | AccountType::Equity | AccountType::Income => Side::Credit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Debit,
    Credit,
}

/// Allotments and obligations are tracked per expense class, and the RAOD
/// registry is maintained one per class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ExpenseClass {
    /// Personal Services.
    Ps,
    /// Maintenance and Other Operating Expenses.
    Mooe,
    /// Financial Expenses.
    Fe,
    /// Capital Outlay.
    Co,
}

/// LGU accounting is fund-based: every transaction belongs to a fund, and the
/// books and statements are kept per fund before consolidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundKind {
    General,
    SpecialEducation,
    Trust,
}

/// The maker–checker lifecycle shared by every approvable document.
///
/// Transitions are enforced server-side; the client only reflects them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Draft,
    Submitted,
    Certified,
    Approved,
    Posted,
    Cancelled,
    Reversed,
}

impl DocumentStatus {
    /// Whether the document is still editable. Posted documents never are —
    /// they are corrected by reversing entries.
    pub fn is_editable(self) -> bool {
        matches!(self, DocumentStatus::Draft | DocumentStatus::Submitted)
    }

    /// How to describe this state to a user, in plain language.
    pub fn label(self) -> &'static str {
        match self {
            DocumentStatus::Draft => "Draft",
            DocumentStatus::Submitted => "For certification",
            DocumentStatus::Certified => "Certified",
            DocumentStatus::Approved => "For posting",
            DocumentStatus::Posted => "Posted",
            DocumentStatus::Cancelled => "Cancelled",
            DocumentStatus::Reversed => "Reversed",
        }
    }
}
