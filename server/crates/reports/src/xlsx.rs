//! Spreadsheet export.
//!
//! The Accounting Office already works in Excel — the forms this system
//! reproduces came out of their own workbook — so an export that lands as a
//! real worksheet, with the figures as *numbers*, is worth more to them than
//! any PDF. A column they cannot sum is a column they will retype.
//!
//! ## On money in a spreadsheet
//!
//! Every amount in GIAMS is an exact decimal, and stays exact everywhere it is
//! computed, stored, or displayed. A spreadsheet cell cannot hold one:
//! Excel's number type is IEEE-754 binary floating point, and there is no
//! format in the file that changes that.
//!
//! So the conversion happens here, at the boundary, deliberately and in one
//! place — never upstream. Two properties make it safe:
//!
//! * Amounts are whole centavos below 2^53, so the *centavo count* converts to
//!   f64 exactly. Dividing by 100 is the only inexact step, and it lands on
//!   the nearest double to the true value.
//! * Cells carry a `0.00` format, so what a clerk reads, and what Excel prints,
//!   is the exact figure the ledger holds.
//!
//! The authoritative figure remains the one in the database. This file is a
//! copy for working with, which is exactly what the office wants it for.

use giams_domain::Money;
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook, Worksheet};

pub use rust_xlsxwriter::XlsxError;

/// A sheet built column by column, in the order the paper form is ruled.
pub struct SheetBuilder {
    workbook: Workbook,
    row: u32,
}

/// How a column's values are written and measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Col {
    /// Free text — particulars, account titles.
    Text(f64),
    /// Codes and reference numbers: text, but never re-interpreted by Excel.
    /// Account code `1-01-02-010` would otherwise be mangled into a date.
    Code(f64),
    /// An exact amount, written as a number so the column can be summed.
    Amount,
}

impl Col {
    fn width(self) -> f64 {
        match self {
            Col::Text(w) | Col::Code(w) => w,
            Col::Amount => 16.0,
        }
    }
}

/// One cell's content.
pub enum Cell<'a> {
    Text(&'a str),
    Code(&'a str),
    Amount(Money),
    Blank,
}

struct Styles {
    heading: Format,
    subheading: Format,
    header: Format,
    text: Format,
    code: Format,
    amount: Format,
    total_text: Format,
    total_amount: Format,
}

impl Styles {
    fn new() -> Self {
        let amount_format = "#,##0.00";

        Styles {
            heading: Format::new().set_bold().set_align(FormatAlign::Center),
            subheading: Format::new().set_align(FormatAlign::Center),
            header: Format::new()
                .set_bold()
                .set_align(FormatAlign::Center)
                .set_align(FormatAlign::VerticalCenter)
                .set_text_wrap()
                .set_border_bottom(FormatBorder::Medium)
                .set_background_color(Color::RGB(0xF1F3F4)),
            text: Format::new(),
            // Kept as text so Excel leaves `1-01-02-010` alone.
            code: Format::new().set_num_format("@"),
            amount: Format::new().set_num_format(amount_format),
            total_text: Format::new().set_bold().set_border_top(FormatBorder::Thin),
            total_amount: Format::new()
                .set_bold()
                .set_num_format(amount_format)
                .set_border_top(FormatBorder::Thin),
        }
    }
}

impl Default for SheetBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SheetBuilder {
    pub fn new() -> Self {
        SheetBuilder { workbook: Workbook::new(), row: 0 }
    }

    /// Starts a sheet with the heading block the COA form carries.
    ///
    /// The same heading as the printed form, for the same reason: a sheet that
    /// leaves this system and lands in somebody's inbox has to say what it is
    /// and what period it covers, or it becomes unattributable within a week.
    pub fn sheet(
        &mut self,
        name: &str,
        title: &str,
        heading_lines: &[String],
        columns: &[Col],
    ) -> Result<&mut Worksheet, XlsxError> {
        let styles = Styles::new();
        let last_col = columns.len().saturating_sub(1) as u16;

        let sheet = self.workbook.add_worksheet();
        // Excel refuses names over 31 characters or containing []:*?/\ — a
        // failure here would lose the whole export, so the name is trimmed
        // rather than passed through and rejected.
        sheet.set_name(sanitise_sheet_name(name))?;

        let mut row = 0u32;
        sheet.merge_range(row, 0, row, last_col, title, &styles.heading)?;
        row += 1;

        for line in heading_lines {
            sheet.merge_range(row, 0, row, last_col, line, &styles.subheading)?;
            row += 1;
        }

        row += 1; // A blank rule between the heading and the columns.

        for (index, column) in columns.iter().enumerate() {
            sheet.set_column_width(index as u16, column.width())?;
        }

        // Everything above the figures repeats at the top of every printed
        // page, so page 4 of a journal is still identifiable on its own.
        sheet.set_repeat_rows(0, row)?;
        sheet.set_freeze_panes(row + 1, 0)?;

        self.row = row;
        Ok(sheet)
    }

    /// Writes the column headings, then leaves the cursor on the first data row.
    pub fn header_row(&mut self, headings: &[&str]) -> Result<(), XlsxError> {
        let styles = Styles::new();
        let row = self.row;
        let sheet = self.current()?;

        for (index, heading) in headings.iter().enumerate() {
            sheet.write_string_with_format(row, index as u16, *heading, &styles.header)?;
        }

        self.row += 1;
        Ok(())
    }

    pub fn row(&mut self, cells: &[Cell<'_>]) -> Result<(), XlsxError> {
        self.write_row(cells, false)
    }

    /// A total, ruled off above as it is on the form.
    pub fn total_row(&mut self, cells: &[Cell<'_>]) -> Result<(), XlsxError> {
        self.write_row(cells, true)
    }

    fn write_row(&mut self, cells: &[Cell<'_>], total: bool) -> Result<(), XlsxError> {
        let styles = Styles::new();
        let row = self.row;
        let sheet = self.current()?;

        for (index, cell) in cells.iter().enumerate() {
            let column = index as u16;
            match cell {
                Cell::Text(value) => sheet.write_string_with_format(
                    row,
                    column,
                    *value,
                    if total { &styles.total_text } else { &styles.text },
                )?,
                Cell::Code(value) => sheet.write_string_with_format(
                    row,
                    column,
                    *value,
                    if total { &styles.total_text } else { &styles.code },
                )?,
                Cell::Amount(amount) => sheet.write_number_with_format(
                    row,
                    column,
                    to_cell_value(*amount),
                    if total { &styles.total_amount } else { &styles.amount },
                )?,
                Cell::Blank => sheet.write_blank(
                    row,
                    column,
                    if total { &styles.total_text } else { &styles.text },
                )?,
            };
        }

        self.row += 1;
        Ok(())
    }

    /// A note in place of figures, when there is nothing to report.
    ///
    /// Says so in words rather than handing over an empty grid, which reads as
    /// a broken export and gets reported as one.
    pub fn note(&mut self, text: &str) -> Result<(), XlsxError> {
        let styles = Styles::new();
        let row = self.row;
        let sheet = self.current()?;
        sheet.write_string_with_format(row, 0, text, &styles.text)?;
        self.row += 1;
        Ok(())
    }

    fn current(&mut self) -> Result<&mut Worksheet, XlsxError> {
        let last = self.workbook.worksheets().len().saturating_sub(1);
        self.workbook
            .worksheet_from_index(last)
            .map_err(|_| XlsxError::UnknownWorksheetNameOrIndex(last.to_string()))
    }

    /// The finished workbook, ready to send.
    pub fn finish(&mut self) -> Result<Vec<u8>, XlsxError> {
        self.workbook.save_to_buffer()
    }
}

/// Converts an exact amount to the nearest double, for one cell.
///
/// The single float conversion in the system, and the only place one is
/// correct: see this module's header for why it is safe and why it does not
/// belong anywhere upstream.
fn to_cell_value(amount: Money) -> f64 {
    use rust_decimal::prelude::ToPrimitive;
    amount.inner().to_f64().unwrap_or(0.0)
}

/// Trims a sheet name to what Excel will accept.
fn sanitise_sheet_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') { ' ' } else { c })
        .collect();

    cleaned.chars().take(31).collect::<String>().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn sheet_names_excel_would_reject_are_trimmed() {
        assert_eq!(sanitise_sheet_name("Budget/Actual"), "Budget Actual");
        assert_eq!(
            sanitise_sheet_name("Statement of Comparison of Budget and Actual Amounts").len(),
            31
        );
    }

    /// The property the module header claims. Worth asserting, because it is
    /// the reason a float conversion is tolerated here at all.
    #[test]
    fn amounts_survive_the_round_trip_to_a_cell() {
        for literal in [
            "0.00", "0.01", "0.05", "1234.56", "99999999.99", "-4567.89", "1000000.10",
        ] {
            let money = Money::new(Decimal::from_str(literal).unwrap());
            let cell = to_cell_value(money);
            assert_eq!(
                format!("{cell:.2}"),
                literal,
                "{literal} does not read back the same from a cell"
            );
        }
    }

    #[test]
    fn a_sheet_with_a_heading_and_a_total_is_a_readable_workbook() {
        let mut builder = SheetBuilder::new();
        builder
            .sheet(
                "General Journal",
                "GENERAL JOURNAL",
                &["Municipality of Gasan".to_string(), "All funds".to_string()],
                &[Col::Code(12.0), Col::Text(40.0), Col::Amount, Col::Amount],
            )
            .unwrap();
        builder.header_row(&["JEV No.", "Particulars", "Debit", "Credit"]).unwrap();
        builder
            .row(&[
                Cell::Code("JEV-2026-08-0001"),
                Cell::Text("Collection of real property tax"),
                Cell::Amount(Money::new(Decimal::from_str("348750.50").unwrap())),
                Cell::Blank,
            ])
            .unwrap();
        builder
            .total_row(&[
                Cell::Text("Total"),
                Cell::Blank,
                Cell::Amount(Money::new(Decimal::from_str("348750.50").unwrap())),
                Cell::Amount(Money::new(Decimal::from_str("348750.50").unwrap())),
            ])
            .unwrap();

        let bytes = builder.finish().unwrap();

        // A zip container, which is what an xlsx is.
        assert_eq!(&bytes[..2], b"PK", "the export is not a spreadsheet file");
        assert!(bytes.len() > 1000, "the export is suspiciously small");
    }
}
