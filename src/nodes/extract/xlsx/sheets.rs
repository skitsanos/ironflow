//! Turning a worksheet's cell range into rows.

use anyhow::{Result, bail};
use calamine::{Data, Range};
use serde_json::Value;

use super::cells::cell_value;
use super::headers::header_keys;

/// Cells still available to this extraction.
///
/// Shared across sheets so a workbook is bounded as a whole. Narrowing with
/// `sheet` therefore lowers the cost, and a workbook too large to read whole
/// can still be read one sheet at a time.
///
/// The limit is taken as a constructor argument rather than read from
/// `IRONFLOW_MAX_XLSX_CELLS` here, so it stays a plain literal in tests and
/// carries no `std::env` access at all — the node's `execute` (Task 7) is
/// expected to pass `crate::util::limits::max_xlsx_cells()` at the call site.
pub(super) struct CellBudget {
    remaining: u64,
    max_cells: u64,
}

impl CellBudget {
    pub(super) fn new(max_cells: u64) -> Self {
        Self {
            remaining: max_cells,
            max_cells,
        }
    }

    /// Charges the budget for the sheet's full cell count: every cell in the
    /// range, including the header row if present. This makes a workbook's
    /// cost a property of the file alone, independent of configuration like
    /// `has_header`, so budget accounting is predictable across calls.
    pub(super) fn spend(&mut self, cells: u64, sheet: &str) -> Result<()> {
        match self.remaining.checked_sub(cells) {
            Some(left) => {
                self.remaining = left;
                Ok(())
            }
            None => bail!(
                "extract_xlsx: sheet '{sheet}' exceeds the remaining cell budget, \
                 with IRONFLOW_MAX_XLSX_CELLS set to {}. Raise that variable, \
                 or set `sheet` to narrow the extraction.",
                self.max_cells
            ),
        }
    }
}

/// Convert one worksheet's range into rows.
///
/// Ceilings are checked here rather than after conversion so an oversized
/// workbook fails before its data reaches the Lua converter — otherwise the
/// failure surfaces later as a conversion node-budget error naming a JSON path
/// rather than a sheet or a file (IF-058).
///
/// `max_rows` is likewise taken as an argument rather than read from
/// `IRONFLOW_MAX_XLSX_ROWS` internally; the node's `execute` (Task 7) passes
/// `crate::util::limits::max_xlsx_rows()`.
pub(super) fn sheet_rows(
    sheet: &str,
    range: &Range<Data>,
    has_header: bool,
    max_rows: u64,
    budget: &mut CellBudget,
) -> Result<Vec<Value>> {
    let height = range.height() as u64;
    if height > max_rows {
        bail!(
            "extract_xlsx: sheet '{sheet}' has {height} rows, exceeding \
             IRONFLOW_MAX_XLSX_ROWS ({max_rows}). Raise that variable, \
             or set `sheet` to narrow the extraction."
        );
    }

    let width = range.width() as u64;
    budget.spend(height.saturating_mul(width), sheet)?;

    let mut source = range.rows();
    let keys = if has_header {
        match source.next() {
            Some(header) => header_keys(header),
            None => return Ok(Vec::new()),
        }
    } else {
        Vec::new()
    };

    let mut rows = Vec::new();
    for row in source {
        rows.push(if has_header {
            let mut object = serde_json::Map::new();
            for (index, cell) in row.iter().enumerate() {
                let key = keys
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("column_{}", index + 1));
                object.insert(key, cell_value(cell));
            }
            Value::Object(object)
        } else {
            Value::Array(row.iter().map(cell_value).collect())
        });
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::{CellBudget, sheet_rows};
    use calamine::{Data, Range};
    use serde_json::json;

    /// Ceilings generous enough that the non-ceiling tests never trip them.
    const MAX_ROWS: u64 = 1_000;
    const MAX_CELLS: u64 = 1_000;

    fn range(rows: Vec<Vec<Data>>) -> Range<Data> {
        if rows.is_empty() {
            return Range::empty();
        }
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut range = Range::new(
            (0, 0),
            (
                rows.len().saturating_sub(1) as u32,
                width.saturating_sub(1) as u32,
            ),
        );
        for (r, row) in rows.into_iter().enumerate() {
            for (c, cell) in row.into_iter().enumerate() {
                range.set_value((r as u32, c as u32), cell);
            }
        }
        range
    }

    fn text(value: &str) -> Data {
        Data::String(value.to_string())
    }

    #[test]
    fn rows_become_objects_keyed_by_the_header() {
        let mut budget = CellBudget::new(MAX_CELLS);
        let rows = sheet_rows(
            "Summary",
            &range(vec![
                vec![text("name"), text("qty")],
                vec![text("Acme"), Data::Float(3.0)],
            ]),
            true,
            MAX_ROWS,
            &mut budget,
        )
        .unwrap();

        assert_eq!(rows, vec![json!({"name": "Acme", "qty": 3})]);
    }

    #[test]
    fn without_a_header_rows_are_arrays_and_the_first_row_is_data() {
        let mut budget = CellBudget::new(MAX_CELLS);
        let rows = sheet_rows(
            "Summary",
            &range(vec![vec![text("name")], vec![text("Acme")]]),
            false,
            MAX_ROWS,
            &mut budget,
        )
        .unwrap();

        assert_eq!(rows, vec![json!(["name"]), json!(["Acme"])]);
    }

    #[test]
    fn a_sparse_row_yields_nulls_rather_than_shifting_columns() {
        let mut budget = CellBudget::new(MAX_CELLS);
        let rows = sheet_rows(
            "Summary",
            &range(vec![
                vec![text("a"), text("b"), text("c")],
                vec![text("x"), Data::Empty, text("z")],
            ]),
            true,
            MAX_ROWS,
            &mut budget,
        )
        .unwrap();

        assert_eq!(rows, vec![json!({"a": "x", "b": null, "c": "z"})]);
    }

    #[test]
    fn an_empty_sheet_yields_no_rows() {
        let mut budget = CellBudget::new(MAX_CELLS);
        assert!(
            sheet_rows("Empty", &range(vec![]), true, MAX_ROWS, &mut budget)
                .unwrap()
                .is_empty()
        );
        // Header-only is also no data rows.
        assert!(
            sheet_rows(
                "HeaderOnly",
                &range(vec![vec![text("a")]]),
                true,
                MAX_ROWS,
                &mut budget
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn the_row_ceiling_names_the_sheet_and_its_override() {
        // The limit is passed in directly, not read from the environment, so
        // this test cannot race any other test over a shared env var.
        let mut budget = CellBudget::new(MAX_CELLS);
        let error = sheet_rows(
            "Q1",
            &range(vec![vec![text("a")], vec![text("1")], vec![text("2")]]),
            true,
            2,
            &mut budget,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("'Q1'"), "{error}");
        assert!(error.contains("IRONFLOW_MAX_XLSX_ROWS"), "{error}");
        assert!(error.contains("sheet"), "{error}");
    }

    #[test]
    fn the_cell_budget_spans_sheets_and_names_its_override() {
        let mut budget = CellBudget::new(4);
        let two_cells = range(vec![vec![text("a"), text("b")], vec![text("1"), text("2")]]);
        // A 2×2 range (header row + 1 data row, 2 columns) costs 4 cells total.
        // First sheet fits: spends exactly 4 of the 4-cell budget, leaving 0.
        // This exercises the "exactly exhausts the budget is accepted" boundary.
        sheet_rows("One", &two_cells, true, MAX_ROWS, &mut budget).unwrap();
        // Second sheet needs 4 cells against 0 remaining and fails.
        let error = sheet_rows("Two", &two_cells, true, MAX_ROWS, &mut budget)
            .unwrap_err()
            .to_string();

        assert!(error.contains("IRONFLOW_MAX_XLSX_CELLS"), "{error}");
        assert!(error.contains("'Two'"), "{error}");
    }
}
