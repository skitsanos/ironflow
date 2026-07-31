use super::super::output_budget::OutputBudget;
use super::super::stream::test_support::{FakeCells, FilteredCells};
use super::{CellBudget, sheet_rows};
use calamine::Data;

/// Ceilings generous enough that the non-ceiling tests never trip them.
const MAX_ROWS: u64 = 1_000;
const MAX_CELLS: u64 = 1_000;
const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;

fn text(value: &str) -> Data {
    Data::String(value.to_string())
}

#[test]
fn rows_become_objects_keyed_by_the_header() {
    let mut budget = CellBudget::new(MAX_CELLS);
    let mut output_budget = OutputBudget::new(MAX_OUTPUT_BYTES);
    let mut source = FakeCells::new(vec![
        vec![text("name"), text("qty")],
        vec![text("Acme"), Data::Float(3.0)],
    ]);
    let rows = sheet_rows(
        "Summary",
        &mut source,
        true,
        MAX_ROWS,
        &mut budget,
        &mut output_budget,
        None,
    )
    .unwrap();

    assert_eq!(rows, vec![serde_json::json!({"name": "Acme", "qty": 3})]);
}

#[test]
fn without_a_header_rows_are_arrays_and_the_first_row_is_data() {
    let mut budget = CellBudget::new(MAX_CELLS);
    let mut output_budget = OutputBudget::new(MAX_OUTPUT_BYTES);
    let mut source = FakeCells::new(vec![vec![text("name")], vec![text("Acme")]]);
    let rows = sheet_rows(
        "Summary",
        &mut source,
        false,
        MAX_ROWS,
        &mut budget,
        &mut output_budget,
        None,
    )
    .unwrap();

    assert_eq!(
        rows,
        vec![serde_json::json!(["name"]), serde_json::json!(["Acme"])]
    );
}

#[test]
fn a_sparse_row_yields_nulls_rather_than_shifting_columns() {
    let mut budget = CellBudget::new(MAX_CELLS);
    let mut output_budget = OutputBudget::new(MAX_OUTPUT_BYTES);
    let mut source = FakeCells::new(vec![
        vec![text("a"), text("b"), text("c")],
        vec![text("x"), Data::Empty, text("z")],
    ]);
    let rows = sheet_rows(
        "Summary",
        &mut source,
        true,
        MAX_ROWS,
        &mut budget,
        &mut output_budget,
        None,
    )
    .unwrap();

    assert_eq!(
        rows,
        vec![serde_json::json!({"a": "x", "b": null, "c": "z"})]
    );
}

#[test]
fn a_data_row_wider_than_the_header_gets_positional_keys() {
    let mut budget = CellBudget::new(MAX_CELLS);
    let mut output_budget = OutputBudget::new(MAX_OUTPUT_BYTES);
    let mut source = FakeCells::new(vec![vec![text("name")], vec![text("Acme"), text("EMEA")]]);
    let rows = sheet_rows(
        "Summary",
        &mut source,
        true,
        MAX_ROWS,
        &mut budget,
        &mut output_budget,
        None,
    )
    .unwrap();

    assert_eq!(
        rows,
        vec![serde_json::json!({"name": "Acme", "column_2": "EMEA"})]
    );
}

#[test]
fn an_empty_sheet_yields_no_rows() {
    let mut budget = CellBudget::new(MAX_CELLS);
    let mut output_budget = OutputBudget::new(MAX_OUTPUT_BYTES);
    assert!(
        sheet_rows(
            "Empty",
            &mut FakeCells::new(vec![]),
            true,
            MAX_ROWS,
            &mut budget,
            &mut output_budget,
            None
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
            &mut budget,
            &mut output_budget,
            None
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
    let mut output_budget = OutputBudget::new(MAX_OUTPUT_BYTES);
    let mut source = FakeCells::new(vec![vec![text("a")], vec![text("1")], vec![text("2")]]);
    let error = sheet_rows(
        "Q1",
        &mut source,
        true,
        2,
        &mut budget,
        &mut output_budget,
        None,
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
    let mut output_budget = OutputBudget::new(MAX_OUTPUT_BYTES);
    let two_cells = || FakeCells::new(vec![vec![text("a"), text("b")], vec![text("1"), text("2")]]);
    // A 2×2 range (header row + 1 data row, 2 columns) costs 4 cells total.
    // First sheet fits: spends exactly 4 of the 4-cell budget, leaving 0.
    // This exercises the "exactly exhausts the budget is accepted" boundary.
    sheet_rows(
        "One",
        &mut two_cells(),
        true,
        MAX_ROWS,
        &mut budget,
        &mut output_budget,
        None,
    )
    .unwrap();
    // The budget is shared across sheets: "One" left 0 remaining, so
    // "Two" fails on the very first cell it streams (a 1×1 bounding box,
    // charged incrementally like every other cell) rather than on some
    // sheet-boundary special case or a declared-dimension pre-check.
    let error = sheet_rows(
        "Two",
        &mut two_cells(),
        true,
        MAX_ROWS,
        &mut budget,
        &mut output_budget,
        None,
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("IRONFLOW_MAX_XLSX_CELLS"), "{error}");
    assert!(error.contains("'Two'"), "{error}");
}

#[test]
fn a_far_corner_is_refused_without_the_process_accumulating_the_bounding_box() {
    // The whole point of streaming: a sheet whose two cells are far
    // apart is refused by the cell budget from the actual positions
    // seen, never by materialising the bounding box between them. Uses
    // the actual Excel-maximum coordinates (row 1,048,576, column
    // XFD/16,384, both 0-based here) -- the exact far-corner shape that
    // used to SIGKILL the process before streaming -- so a future
    // regression that reintroduces eager bounding-box allocation blows
    // this test up instead of quietly passing it. `max_rows` is passed
    // as a huge literal, not the shared `MAX_ROWS`, specifically so the
    // row ceiling (row 1,048,575 would trip `MAX_ROWS` = 1,000 long
    // before the cell budget got a look-in) never fires here -- this
    // test is about the cell budget alone.
    let mut budget = CellBudget::new(100);
    let mut output_budget = OutputBudget::new(MAX_OUTPUT_BYTES);
    let mut source = FakeCells::positioned(vec![
        ((0, 0), text("a")),
        ((1_048_575, 16_383), text("far")),
    ]);
    let error = sheet_rows(
        "Big",
        &mut source,
        false,
        2_000_000,
        &mut budget,
        &mut output_budget,
        None,
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("IRONFLOW_MAX_XLSX_CELLS"), "{error}");
    assert!(error.contains("'Big'"), "{error}");
}

#[test]
fn a_formatting_only_empty_cell_does_not_widen_the_row() {
    // Mirrors a real `<c r="Z2" s="1"/>` -- a formatting-only cell, or a
    // merged span's non-anchor cell -- that calamine's cell reader still
    // yields even though `worksheet_range` never does. `FakeCells::positioned`
    // preserves that `Data::Empty` verbatim, and `FilteredCells` wraps it
    // with the exact same `skip_empty` filter production runs (see
    // `stream.rs`), so this proves the real fix keeps the row two columns
    // wide instead of stretching out to column 26 the way the regression
    // this review caught did.
    let mut budget = CellBudget::new(MAX_CELLS);
    let mut output_budget = OutputBudget::new(MAX_OUTPUT_BYTES);
    let mut source = FilteredCells(FakeCells::positioned(vec![
        ((0, 0), text("name")),
        ((0, 1), text("qty")),
        ((1, 0), text("Acme")),
        ((1, 1), Data::Float(3.0)),
        ((1, 25), Data::Empty), // e.g. <c r="Z2" s="1"/>
    ]));
    let rows = sheet_rows(
        "Summary",
        &mut source,
        true,
        MAX_ROWS,
        &mut budget,
        &mut output_budget,
        None,
    )
    .unwrap();

    assert_eq!(rows, vec![serde_json::json!({"name": "Acme", "qty": 3})]);
}

#[path = "tests/output_budget_tests.rs"]
mod output_budget_tests;

// An overstating `<dimension>` (e.g. `A1:BH50000` on a sheet whose real
// content is 100 rows x 5 columns) is no longer capable of rejecting
// anything at all: `CellSource` doesn't expose a declared dimension any
// more (Finding 2), so there is nothing here to construct a unit test
// against. `tests/test_extract_xlsx.rs`'s
// `an_overstating_dimension_does_not_cause_a_false_rejection` covers this
// at the integration level, through a real `<dimension>` written into an
// actual `.xlsx` file.
