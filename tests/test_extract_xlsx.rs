#[path = "support/xlsx.rs"]
mod xlsx_support;

use xlsx_support::{
    SheetSpec, boolean, date_builtin, date_custom, error, number, row, text, write_workbook,
};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use ironflow::util::execution::with_execution_deadline;

#[test]
fn the_fixture_builder_produces_a_workbook_calamine_can_open() {
    // Everything downstream rests on this: if the builder emitted subtly
    // invalid OOXML, every later test would fail for the wrong reason.
    use calamine::Reader;

    let dir = tempfile::tempdir().unwrap();
    let path = write_workbook(
        dir.path(),
        "probe.xlsx",
        &[
            SheetSpec::new(
                "Summary",
                row(1, &[text("A1", "name"), text("B1", "region")]),
            ),
            SheetSpec::new("Q1", row(1, &[text("A1", "q")])),
        ],
    );

    let mut workbook: calamine::Xlsx<_> = calamine::open_workbook(&path).unwrap();
    assert_eq!(workbook.sheet_names(), &["Summary", "Q1"]);

    let range = workbook.worksheet_range("Summary").unwrap();
    assert_eq!(range.height(), 1);
    assert_eq!(range.width(), 2);
}

/// Execute the registered `extract_xlsx` node, matching how the other extract
/// tests in `test_extract_nodes.rs` invoke nodes via `NodeRegistry`.
async fn run_node(config: serde_json::Value) -> anyhow::Result<Context> {
    let node = NodeRegistry::with_builtins().get("extract_xlsx").unwrap();
    node.execute(&config, &Context::new()).await
}

fn two_sheet_workbook(dir: &std::path::Path) -> std::path::PathBuf {
    write_workbook(
        dir,
        "book.xlsx",
        &[
            SheetSpec::new(
                "Summary",
                format!(
                    "{}{}",
                    row(
                        1,
                        &[
                            text("A1", "region"),
                            text("B1", "revenue"),
                            text("C1", "signed"),
                            text("D1", "active"),
                            text("E1", "broken"),
                            text("F1", "signed_builtin"),
                        ]
                    ),
                    row(
                        2,
                        &[
                            text("A2", "EU"),
                            number("B2", "1200"),
                            date_custom("C2", "46237"),
                            boolean("D2", "1"),
                            error("E2", "#DIV/0!"),
                            date_builtin("F2", "46237"),
                        ]
                    ),
                ),
            ),
            SheetSpec::new(
                "Q1",
                format!(
                    "{}{}",
                    row(1, &[text("A1", "region")]),
                    row(2, &[text("A2", "US")]),
                ),
            ),
        ],
    )
}

#[tokio::test]
async fn every_sheet_is_returned_keyed_by_name_with_an_ordered_name_list() {
    let dir = tempfile::tempdir().unwrap();
    let path = two_sheet_workbook(dir.path());

    let out = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap();

    assert_eq!(
        out["content_sheet_names"],
        serde_json::json!(["Summary", "Q1"]),
        "names must be in workbook order for deterministic foreach"
    );
    assert!(out["content"].get("Summary").is_some());
    assert!(out["content"].get("Q1").is_some());
}

#[tokio::test]
async fn cells_arrive_typed_with_dates_as_iso_strings() {
    let dir = tempfile::tempdir().unwrap();
    let path = two_sheet_workbook(dir.path());

    let out = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap();
    let row = &out["content"]["Summary"][0];

    assert_eq!(row["region"], serde_json::json!("EU"));
    assert_eq!(
        row["revenue"],
        serde_json::json!(1200),
        "whole numbers must not arrive as 1200.0"
    );
    assert_eq!(row["signed"], serde_json::json!("2026-08-03T00:00:00"));
    assert_eq!(row["active"], serde_json::json!(true));
    assert_eq!(
        row["broken"],
        serde_json::json!(null),
        "an Excel error is null, like a blank"
    );
    // Excel actually writes the built-in `numFmtId="14"` for a plain date
    // cell, not a custom format code -- both must produce the same ISO-8601
    // output.
    assert_eq!(
        row["signed_builtin"],
        serde_json::json!("2026-08-03T00:00:00"),
        "a built-in date format (numFmtId 14) must format identically to a custom one"
    );
}

#[tokio::test]
async fn sheet_narrows_by_name_and_by_index_without_changing_the_shape() {
    let dir = tempfile::tempdir().unwrap();
    let path = two_sheet_workbook(dir.path());

    for selector in [serde_json::json!("Q1"), serde_json::json!(1)] {
        let out = run_node(serde_json::json!({
            "path": path.to_str().unwrap(),
            "sheet": selector,
        }))
        .await
        .unwrap();

        // Still keyed by sheet name, so downstream code never branches on
        // whether `sheet` was used.
        assert_eq!(out["content"]["Q1"][0]["region"], serde_json::json!("US"));
        assert!(out["content"].get("Summary").is_none());
        assert_eq!(out["content_sheet_names"], serde_json::json!(["Q1"]));
    }
}

#[tokio::test]
async fn an_unknown_sheet_errors_and_lists_the_real_ones() {
    let dir = tempfile::tempdir().unwrap();
    let path = two_sheet_workbook(dir.path());

    let error = run_node(serde_json::json!({
        "path": path.to_str().unwrap(),
        "sheet": "Nope",
    }))
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("Nope"), "{error}");
    assert!(error.contains("Summary"), "{error}");
    assert!(error.contains("Q1"), "{error}");
}

#[tokio::test]
async fn an_out_of_range_index_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = two_sheet_workbook(dir.path());

    assert!(
        run_node(serde_json::json!({ "path": path.to_str().unwrap(), "sheet": 9 }))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn has_header_false_returns_arrays_including_the_first_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = two_sheet_workbook(dir.path());

    let out = run_node(serde_json::json!({
        "path": path.to_str().unwrap(),
        "sheet": "Q1",
        "has_header": false,
    }))
    .await
    .unwrap();

    assert_eq!(out["content"]["Q1"][0], serde_json::json!(["region"]));
    assert_eq!(out["content"]["Q1"][1], serde_json::json!(["US"]));
}

#[tokio::test]
async fn output_key_is_honoured() {
    let dir = tempfile::tempdir().unwrap();
    let path = two_sheet_workbook(dir.path());

    let out = run_node(serde_json::json!({
        "path": path.to_str().unwrap(),
        "output_key": "book",
    }))
    .await
    .unwrap();

    assert!(out.contains_key("book"));
    assert!(out.contains_key("book_sheet_names"));
}

#[tokio::test]
async fn a_missing_file_errors() {
    assert!(
        run_node(serde_json::json!({ "path": "/nonexistent/nope.xlsx" }))
            .await
            .is_err()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_fifo_without_a_writer_is_rejected_before_zip_parsing() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workbook.pipe");
    let path_c = CString::new(path.as_os_str().as_bytes()).unwrap();
    let created = unsafe { libc::mkfifo(path_c.as_ptr(), 0o600) };
    assert_eq!(
        created,
        0,
        "mkfifo failed: {}",
        std::io::Error::last_os_error()
    );

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        run_node(serde_json::json!({ "path": path.to_str().unwrap() })),
    )
    .await
    .expect("extract_xlsx blocked while opening a FIFO");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("not a regular file"), "{error}");
}

#[tokio::test]
async fn a_scoped_deadline_stops_the_blocking_parser_before_work_begins() {
    let dir = tempfile::tempdir().unwrap();
    let path = two_sheet_workbook(dir.path());
    let expired = tokio::time::Instant::now() - std::time::Duration::from_millis(1);

    let error = with_execution_deadline(
        Some(expired),
        run_node(serde_json::json!({ "path": path.to_str().unwrap() })),
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("step deadline exceeded"), "{error}");
}

#[tokio::test]
async fn supplying_both_path_and_source_key_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = two_sheet_workbook(dir.path());

    assert!(
        run_node(serde_json::json!({
            "path": path.to_str().unwrap(),
            "source_key": "somewhere",
        }))
        .await
        .is_err()
    );
}

#[tokio::test]
async fn a_merged_cell_reports_its_value_top_left_and_null_across_the_span() {
    // A merge is not a cell type: the file stores the value in the top-left
    // and leaves the rest of the span empty. The node reports the file rather
    // than filling the span.
    let dir = tempfile::tempdir().unwrap();
    let path = write_workbook(
        dir.path(),
        "merged.xlsx",
        &[SheetSpec::new(
            "M",
            format!(
                "{}{}",
                row(1, &[text("A1", "left"), text("B1", "right")]),
                // B2 omitted entirely, as a merged span stores it.
                row(2, &[text("A2", "spans both")]),
            ),
        )],
    );

    let out = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap();

    assert_eq!(
        out["content"]["M"][0]["left"],
        serde_json::json!("spans both")
    );
    assert_eq!(out["content"]["M"][0]["right"], serde_json::json!(null));
}

#[tokio::test]
async fn a_workbook_with_a_far_corner_is_refused_by_the_cell_budget_not_by_memory() {
    // `calamine`'s `worksheet_range` would materialise a dense array over the
    // *bounding box* of these two cells -- 60 x 50,000 -- rather than the 2
    // real values the file holds. The node must reject this via the cell
    // budget and the process must stay alive; it must never attempt that
    // allocation. Assert on the error, not on memory.
    let dir = tempfile::tempdir().unwrap();
    let path = write_workbook(
        dir.path(),
        "far_corner.xlsx",
        &[SheetSpec::new(
            "Big",
            format!(
                "{}{}",
                row(1, &[text("A1", "x")]),
                row(50000, &[number("BH50000", "1")]),
            ),
        )],
    );

    let error = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap_err()
        .to_string();

    // Not `IRONFLOW_MAX_XLSX_CELLS` specifically: row 50,000 sits one row
    // below the *default* `IRONFLOW_MAX_XLSX_ROWS`, so on a machine that
    // exports a lower value this legitimately fails via the row ceiling
    // instead. Both ceiling errors share this closing sentence, so asserting
    // on it is robust to either default being overridden in the environment.
    assert!(
        error.contains("Raise that variable, or set `sheet` to narrow the extraction."),
        "{error}"
    );
}

#[tokio::test]
async fn a_declared_dimension_that_understates_the_content_does_not_bypass_the_budget() {
    // The sheet declares `A1:A1` -- a single cell -- while actually holding
    // one far away, at row 100 (well under IRONFLOW_MAX_XLSX_ROWS, so this
    // exercises the cell budget specifically rather than the row ceiling)
    // and column ZZ (the 702nd column). A guard that trusted the declared
    // dimension alone would accept this file (bounding box 1x1); only the
    // streaming counters, built from the cells' real positions -- 100 rows x
    // 702 cols here -- catch it. This is exactly why streaming was chosen
    // over a declared-dimension check.
    let dir = tempfile::tempdir().unwrap();
    let path = write_workbook(
        dir.path(),
        "understated_dimension.xlsx",
        &[SheetSpec::new(
            "Big",
            format!(
                "{}{}",
                row(1, &[text("A1", "x")]),
                row(100, &[number("ZZ100", "1")]),
            ),
        )
        .with_dimension("A1:A1")],
    );

    let error = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap_err()
        .to_string();

    // Same robustness note as the far-corner test above: assert on the
    // sentence both ceiling errors share rather than the cell-budget
    // variable name specifically, so an environment with a lower
    // `IRONFLOW_MAX_XLSX_ROWS` can't fail this for the wrong reason.
    assert!(
        error.contains("Raise that variable, or set `sheet` to narrow the extraction."),
        "{error}"
    );
}

#[tokio::test]
async fn an_overstating_dimension_does_not_cause_a_false_rejection() {
    // The mirror image of the test above: a declared `<dimension>` that
    // *overstates* the sheet's real content -- `A1:BH50000`, 60 columns by
    // 50,000 rows -- while the sheet actually holds 3 rows x 2 real columns.
    // A guard that trusted (or even just cheaply pre-checked) the declared
    // dimension would refuse this outright, before reading a single cell,
    // because 50,000 alone already meets the default `IRONFLOW_MAX_XLSX_ROWS`.
    // The declared `<dimension>` is not consulted for rejection at all any
    // more (Finding 2): only the streamed counters, built from the 3x2 real
    // cells, decide -- and those are nowhere near either ceiling, so this
    // must extract cleanly.
    let dir = tempfile::tempdir().unwrap();
    let path = write_workbook(
        dir.path(),
        "overstated_dimension.xlsx",
        &[SheetSpec::new(
            "Small",
            format!(
                "{}{}",
                row(1, &[text("A1", "name"), text("B1", "qty")]),
                row(2, &[text("A2", "Acme"), number("B2", "3")]),
            ),
        )
        .with_dimension("A1:BH50000")],
    );

    let out = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap();

    assert_eq!(
        out["content"]["Small"][0],
        serde_json::json!({"name": "Acme", "qty": 3})
    );
}

#[tokio::test]
async fn a_formula_yields_its_cached_value_not_the_expression() {
    // IronFlow does not evaluate formulas; it reports what Excel last stored.
    let dir = tempfile::tempdir().unwrap();
    let path = write_workbook(
        dir.path(),
        "formula.xlsx",
        &[SheetSpec::new(
            "F",
            format!(
                "{}{}",
                row(1, &[text("A1", "total")]),
                r#"<row r="2"><c r="A2"><f>1+1</f><v>2</v></c></row>"#,
            ),
        )],
    );

    let out = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap();

    assert_eq!(out["content"]["F"][0]["total"], serde_json::json!(2));
}

#[tokio::test]
#[ignore = "pre-fix this SIGKILLs the test process rather than failing an \
            assertion, which is unfriendly to CI; run manually with \
            `cargo test -- --ignored` to re-verify the fix"]
async fn a1_and_xfd1048576_is_the_input_that_actually_separates_the_implementations() {
    // This is *the* repro that distinguishes the pre-streaming implementation
    // from the current one, preserved here so it is not lost to the record
    // even though it cannot run in ordinary CI.
    //
    // Two real cells, `A1` and `XFD1048576` -- Excel's actual maximum corner,
    // giving a bounding box of 16,384 x 1,048,576 cells:
    //
    // - Pre-fix (`calamine::Xlsx::worksheet_range`, which materialises a
    //   dense `Vec` sized to that bounding box): this allocates roughly
    //   550 GB and the OS SIGKILLs the process. There is no clean error to
    //   assert on -- the process is simply gone.
    // - Post-fix (streaming via `worksheet_cells_reader`): instantaneous and
    //   allocation-free. The second cell's row index (1,048,575, 0-based)
    //   trips `IRONFLOW_MAX_XLSX_ROWS` (default 50,000) the moment it
    //   arrives, before any bounding box is grown for it, so the failure is
    //   a plain error rather than an allocation.
    let dir = tempfile::tempdir().unwrap();
    let path = write_workbook(
        dir.path(),
        "far_corner_max.xlsx",
        &[SheetSpec::new(
            "Big",
            format!(
                "{}{}",
                row(1, &[text("A1", "x")]),
                row(1_048_576, &[text("XFD1048576", "far")]),
            ),
        )],
    );

    let started = std::time::Instant::now();
    let error = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap_err()
        .to_string();
    let elapsed = started.elapsed();

    assert!(error.contains("IRONFLOW_MAX_XLSX_ROWS"), "{error}");
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "took {elapsed:?}; the whole point is that this no longer allocates"
    );
}
