#[path = "support/xlsx.rs"]
mod xlsx_support;

use xlsx_support::{SheetSpec, boolean, date_custom, error, number, row, text, write_workbook};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

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
