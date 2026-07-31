use super::{CellBudget, FakeCells, MAX_ROWS, OutputBudget, sheet_rows, text};

#[test]
fn repeated_large_strings_trip_the_byte_budget_per_cell_reference() {
    // This models one large `sharedStrings.xml` entry referenced by several
    // cells. The archive contains one copy, but calamine/output construction
    // clone it per reference; the byte budget must therefore fail during the
    // third decoded use rather than treating all references as free.
    let shared = "x".repeat(1_024);
    let mut source = FakeCells::new(vec![
        vec![text(&shared)],
        vec![text(&shared)],
        vec![text(&shared)],
    ]);
    let mut cell_budget = CellBudget::new(10);
    let mut output_budget = OutputBudget::new(2_048);

    let error = sheet_rows(
        "Amplified",
        &mut source,
        false,
        MAX_ROWS,
        &mut cell_budget,
        &mut output_budget,
        None,
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES"), "{error}");
    assert!(error.contains("'Amplified'"), "{error}");
    assert!(error.contains("3072"), "{error}");
}

#[test]
fn result_construction_shares_the_same_budget_as_decoding() {
    // Two four-byte cells consume 8 bytes while decoding. The first array
    // then consumes 3 structural bytes plus its four-byte result, and the
    // second array's structure reaches an exact 18-byte budget. Creating its
    // value must fail at 22 rather than resetting the counter after parsing.
    let mut source = FakeCells::new(vec![vec![text("data")], vec![text("data")]]);
    let mut cell_budget = CellBudget::new(10);
    let mut output_budget = OutputBudget::new(18);

    let error = sheet_rows(
        "Cumulative",
        &mut source,
        false,
        MAX_ROWS,
        &mut cell_budget,
        &mut output_budget,
        None,
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES"), "{error}");
    assert!(error.contains("22"), "{error}");
}
