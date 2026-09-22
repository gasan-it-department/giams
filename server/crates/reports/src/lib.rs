//! Report generation — registries, books, and the six GPFS statements.
//!
//! Spreadsheet export is built ([`xlsx`]). PDF is the browser's print
//! dialog for now — the screens carry the COA form layout and a print
//! stylesheet, so a page prints as the form. Typst templates replace that
//! when the office needs unattended batch PDF generation.
//!
//! Planned approach (docs/ARCHITECTURE.md §7): Typst templates, one per COA
//! form, rendered to PDF; `rust_xlsxwriter` for spreadsheet exports. Typst is
//! a single static binary, handles form-heavy government layouts far better
//! than HTML-to-PDF, and needs no Chromium on the office server.
//!
//! Every statement gets a golden-file test: a fixture dataset with a
//! hand-checked expected output committed to the repository. A financial
//! statement that changes silently is a defect, and without golden files that
//! change is invisible until an auditor finds it.

#![forbid(unsafe_code)]

pub mod xlsx;

/// The complete set of general purpose financial statements required under the
/// GAM for LGUs. Prepared per fund, then consolidated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Statement {
    /// Statement of Financial Position — the balance sheet.
    FinancialPosition,
    /// Statement of Financial Performance — income and expenses.
    FinancialPerformance,
    ChangesInNetAssets,
    CashFlows,
    /// Statement of Comparison of Budget and Actual Amounts. Unique to the
    /// public sector and the one COA scrutinises most for LGUs, because it is
    /// what ties the appropriation ordinance to what was actually spent.
    ComparisonOfBudgetAndActual,
    Notes,
}

impl Statement {
    pub fn title(self) -> &'static str {
        match self {
            Statement::FinancialPosition => "Statement of Financial Position",
            Statement::FinancialPerformance => "Statement of Financial Performance",
            Statement::ChangesInNetAssets => "Statement of Changes in Net Assets/Equity",
            Statement::CashFlows => "Statement of Cash Flows",
            Statement::ComparisonOfBudgetAndActual => {
                "Statement of Comparison of Budget and Actual Amounts"
            }
            Statement::Notes => "Notes to Financial Statements",
        }
    }

    /// All six, in the order COA expects them bound.
    pub fn all() -> [Statement; 6] {
        [
            Statement::FinancialPosition,
            Statement::FinancialPerformance,
            Statement::ChangesInNetAssets,
            Statement::CashFlows,
            Statement::ComparisonOfBudgetAndActual,
            Statement::Notes,
        ]
    }
}
