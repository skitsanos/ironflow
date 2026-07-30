//! Turning a worksheet's streamed cells into rows.
//!
//! Cells arrive one at a time, in file order, via `CellSource` (see
//! `stream.rs`) rather than as a pre-built dense grid — see that module's
//! doc comment for why `calamine::Xlsx::worksheet_range` cannot be used here.
//! `sheet_rows` reconstructs the same shape a dense range would have
//! produced (rows keyed by header, sparse cells as `null`) by tracking the
//! running bounding box of the positions actually seen, without ever
//! allocating an array sized to it ahead of time.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use calamine::Data;
use serde_json::Value;

use super::cells::cell_value;
use super::headers::header_keys;
use super::stream::CellSource;

pub(super) use super::budget::CellBudget;

fn row_ceiling_error(sheet: &str, rows: u64, max_rows: u64) -> anyhow::Error {
    anyhow::anyhow!(
        "extract_xlsx: sheet '{sheet}' has at least {rows} rows, exceeding \
         IRONFLOW_MAX_XLSX_ROWS ({max_rows}). Raise that variable, or set \
         `sheet` to narrow the extraction."
    )
}

/// The bounding-box area of `rows × cols`, computed from inclusive
/// `(start, end)` spans without risking overflow on the intermediate `u32`
/// arithmetic.
fn area(row_span: (u32, u32), col_span: (u32, u32)) -> u64 {
    let height = (row_span.1 - row_span.0) as u64 + 1;
    let width = (col_span.1 - col_span.0) as u64 + 1;
    height.saturating_mul(width)
}

/// Grow an inclusive `(start, end)` span to also cover `index`.
fn grow(span: Option<(u32, u32)>, index: u32) -> (u32, u32) {
    match span {
        Some((start, end)) => (start.min(index), end.max(index)),
        None => (index, index),
    }
}

/// Convert one worksheet's streamed cells into rows.
///
/// Ceilings are enforced while streaming, before a single row of output is
/// built, so an oversized workbook fails before its data reaches the Lua
/// converter — otherwise the failure would surface later as a conversion
/// node-budget error naming a JSON path rather than a sheet or a file
/// (IF-058). The row ceiling is checked against each cell's raw row index as
/// it arrives; the cell ceiling is checked against the sheet's running
/// bounding-box area, which only ever grows, so a bail here means the sheet
/// truly cannot fit — no cells for it have already reached the caller.
pub(super) fn sheet_rows<S: CellSource>(
    sheet: &str,
    source: &mut S,
    has_header: bool,
    max_rows: u64,
    budget: &mut CellBudget,
) -> Result<Vec<Value>> {
    // A cheap early rejection using the declared `<dimension>`, when it is
    // already honest enough to exceed a ceiling on its own — a free win for
    // an oversized workbook that doesn't lie about its size. It is never
    // trusted as the bound itself: an absent `<dimension>` collapses to
    // `len() == 1`, indistinguishable from a genuine one-cell sheet, and a
    // present one can just as easily understate the truth.
    let declared = source.declared_dimensions();
    let declared_rows = (declared.end.0 - declared.start.0) as u64 + 1;
    if declared_rows > max_rows {
        bail!(row_ceiling_error(sheet, declared_rows, max_rows));
    }
    budget.reject_if_over(sheet, declared.len())?;

    let mut by_row: BTreeMap<u32, BTreeMap<u32, Data>> = BTreeMap::new();
    let mut rows_span: Option<(u32, u32)> = None;
    let mut cols_span: Option<(u32, u32)> = None;
    let mut charged: u64 = 0;

    while let Some(((row, col), value)) = source.next_cell()? {
        if row as u64 >= max_rows {
            bail!(row_ceiling_error(sheet, row as u64 + 1, max_rows));
        }

        rows_span = Some(grow(rows_span, row));
        cols_span = Some(grow(cols_span, col));

        let total = area(rows_span.unwrap(), cols_span.unwrap());
        budget.charge(total.saturating_sub(charged), total, sheet)?;
        charged = total;

        by_row.entry(row).or_default().insert(col, value);
    }

    let (Some(rows_span), Some(cols_span)) = (rows_span, cols_span) else {
        return Ok(Vec::new());
    };

    let mut dense_rows = (rows_span.0..=rows_span.1).map(|row| {
        let stored = by_row.get(&row);
        (cols_span.0..=cols_span.1)
            .map(|col| {
                stored
                    .and_then(|cells| cells.get(&col))
                    .cloned()
                    .unwrap_or(Data::Empty)
            })
            .collect::<Vec<Data>>()
    });

    let keys = if has_header {
        match dense_rows.next() {
            Some(header) => header_keys(&header),
            None => return Ok(Vec::new()),
        }
    } else {
        Vec::new()
    };

    let mut rows = Vec::new();
    for row in dense_rows {
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
    use super::super::stream::test_support::FakeCells;
    use super::{CellBudget, sheet_rows};
    use calamine::Data;

    /// Ceilings generous enough that the non-ceiling tests never trip them.
    const MAX_ROWS: u64 = 1_000;
    const MAX_CELLS: u64 = 1_000;

    fn text(value: &str) -> Data {
        Data::String(value.to_string())
    }

    #[test]
    fn rows_become_objects_keyed_by_the_header() {
        let mut budget = CellBudget::new(MAX_CELLS);
        let mut source = FakeCells::new(vec![
            vec![text("name"), text("qty")],
            vec![text("Acme"), Data::Float(3.0)],
        ]);
        let rows = sheet_rows("Summary", &mut source, true, MAX_ROWS, &mut budget).unwrap();

        assert_eq!(rows, vec![serde_json::json!({"name": "Acme", "qty": 3})]);
    }

    #[test]
    fn without_a_header_rows_are_arrays_and_the_first_row_is_data() {
        let mut budget = CellBudget::new(MAX_CELLS);
        let mut source = FakeCells::new(vec![vec![text("name")], vec![text("Acme")]]);
        let rows = sheet_rows("Summary", &mut source, false, MAX_ROWS, &mut budget).unwrap();

        assert_eq!(
            rows,
            vec![serde_json::json!(["name"]), serde_json::json!(["Acme"])]
        );
    }

    #[test]
    fn a_sparse_row_yields_nulls_rather_than_shifting_columns() {
        let mut budget = CellBudget::new(MAX_CELLS);
        let mut source = FakeCells::new(vec![
            vec![text("a"), text("b"), text("c")],
            vec![text("x"), Data::Empty, text("z")],
        ]);
        let rows = sheet_rows("Summary", &mut source, true, MAX_ROWS, &mut budget).unwrap();

        assert_eq!(
            rows,
            vec![serde_json::json!({"a": "x", "b": null, "c": "z"})]
        );
    }

    #[test]
    fn an_empty_sheet_yields_no_rows() {
        let mut budget = CellBudget::new(MAX_CELLS);
        assert!(
            sheet_rows(
                "Empty",
                &mut FakeCells::new(vec![]),
                true,
                MAX_ROWS,
                &mut budget
            )
            .unwrap()
            .is_empty()
        );
        // Header-only is also no data rows.
        assert!(
            sheet_rows(
                "HeaderOnly",
                &mut FakeCells::new(vec![vec![text("a")]]),
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
        let mut source = FakeCells::new(vec![vec![text("a")], vec![text("1")], vec![text("2")]]);
        let error = sheet_rows("Q1", &mut source, true, 2, &mut budget)
            .unwrap_err()
            .to_string();

        assert!(error.contains("'Q1'"), "{error}");
        assert!(error.contains("IRONFLOW_MAX_XLSX_ROWS"), "{error}");
        assert!(error.contains("sheet"), "{error}");
    }

    #[test]
    fn the_cell_budget_spans_sheets_and_names_its_override() {
        let mut budget = CellBudget::new(4);
        let two_cells =
            || FakeCells::new(vec![vec![text("a"), text("b")], vec![text("1"), text("2")]]);
        // A 2×2 range (header row + 1 data row, 2 columns) costs 4 cells total.
        // First sheet fits: spends exactly 4 of the 4-cell budget, leaving 0.
        // This exercises the "exactly exhausts the budget is accepted" boundary.
        sheet_rows("One", &mut two_cells(), true, MAX_ROWS, &mut budget).unwrap();
        // Second sheet needs 4 cells against 0 remaining and fails.
        let error = sheet_rows("Two", &mut two_cells(), true, MAX_ROWS, &mut budget)
            .unwrap_err()
            .to_string();

        assert!(error.contains("IRONFLOW_MAX_XLSX_CELLS"), "{error}");
        assert!(error.contains("'Two'"), "{error}");
    }

    #[test]
    fn a_far_corner_is_refused_without_the_process_accumulating_the_bounding_box() {
        // The whole point of streaming: a sheet whose two cells are far
        // apart is refused by the cell budget from the actual positions
        // seen, never by materialising the bounding box between them.
        let mut budget = CellBudget::new(100);
        let mut far_row = vec![Data::Empty; 199];
        far_row.push(text("far"));
        let mut source = FakeCells::new(vec![vec![text("a")], far_row]);
        let error = sheet_rows("Big", &mut source, false, MAX_ROWS, &mut budget)
            .unwrap_err()
            .to_string();

        assert!(error.contains("IRONFLOW_MAX_XLSX_CELLS"), "{error}");
        assert!(error.contains("'Big'"), "{error}");
    }
}
