//! `extract_xlsx` must bound what calamine builds from `xl/styles.xml` when it
//! opens a workbook (IF-173).
//!
//! Since calamine 0.38 every `<xf>` becomes an owned `Style` holding private
//! copies of its font name and number-format code. The part stores each
//! string once; the open copies it once per cell format.

#[path = "support/xlsx.rs"]
mod xlsx_support;

use xlsx_support::{SheetSpec, date_custom, number, row, write_workbook_with_styles};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

async fn run_node(config: serde_json::Value) -> anyhow::Result<Context> {
    let node = NodeRegistry::with_builtins().get("extract_xlsx").unwrap();
    node.execute(&config, &Context::new()).await
}

/// A style sheet with one font and one custom number format, referenced by
/// `formats` cell formats. Format 0 is general; the rest use the custom code.
fn styles(font_name: &str, format_code: &str, formats: usize) -> String {
    let mut xml = format!(
        r#"<styleSheet xmlns="{NS}"><numFmts count="1"><numFmt numFmtId="164" formatCode="{format_code}"/></numFmts><fonts count="1"><font><sz val="11"/><name val="{font_name}"/></font></fonts><cellXfs count="{formats}"><xf numFmtId="0" fontId="0"/>"#
    );
    for _ in 1..formats {
        xml.push_str(r#"<xf numFmtId="164" fontId="0"/>"#);
    }
    xml.push_str("</cellXfs></styleSheet>");
    xml
}

fn one_cell() -> [SheetSpec; 1] {
    [SheetSpec::new("Data", row(1, &[number("A1", "1")]))]
}

fn assert_refused_by_the_style_bound(error: &anyhow::Error) {
    let message = error.to_string();
    assert!(message.contains("extract_xlsx"), "{message}");
    // The style bound, not the per-part size cap that names the same variable.
    assert!(
        message.contains("styles.xml holds") && message.contains("style records"),
        "{message}"
    );
    assert!(
        message.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES"),
        "{message}"
    );
    assert!(
        message.len() < 1024,
        "error text was {} bytes",
        message.len()
    );
}

#[tokio::test]
async fn strings_repeated_per_cell_format_are_refused_before_the_workbook_opens() {
    // 2,000 formats x (64 KiB font name + 64 KiB format code): about 190 KiB
    // of XML in a 2 KB archive that calamine 0.38 expands to 250 MiB at open.
    let dir = tempfile::tempdir().unwrap();
    let long = "n".repeat(64 * 1024);
    let path = write_workbook_with_styles(
        dir.path(),
        "repeated_strings.xlsx",
        &one_cell(),
        &styles(&long, &long, 2_000),
    );
    assert!(
        std::fs::metadata(&path).unwrap().len() < 64 * 1024,
        "the fixture must stay a small archive"
    );

    let error = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .expect_err("a style table that multiplies at open must be refused");

    assert_refused_by_the_style_bound(&error);
}

#[tokio::test]
async fn a_million_cell_formats_are_refused_by_their_record_cost() {
    // No long string at all: each of these records still costs calamine a
    // 152-byte `Style`, about 150 MiB for a 150 KiB archive. At 31 bytes a
    // record the part stays under the 50 MiB per-part ceiling, so only the
    // style bound can refuse it.
    let dir = tempfile::tempdir().unwrap();
    let styles_xml = styles("Calibri", "0.00", 1_000_000);
    assert!(styles_xml.len() < 40 * 1024 * 1024);
    let path =
        write_workbook_with_styles(dir.path(), "many_formats.xlsx", &one_cell(), &styles_xml);

    let error = run_node(serde_json::json!({ "path": path.to_str().unwrap() }))
        .await
        .expect_err("a million cell formats must be refused");

    assert_refused_by_the_style_bound(&error);
}

#[tokio::test]
async fn a_workbook_at_excels_cell_format_limit_is_still_extracted() {
    // Excel allows 65,490 cell formats, a 255-character format code and a
    // 31-character font name. A workbook at all three limits is legitimate.
    let dir = tempfile::tempdir().unwrap();
    let format_code = format!("yyyy\\-mm\\-dd{}", ";".repeat(255 - 12));
    assert_eq!(format_code.len(), 255);
    let path = write_workbook_with_styles(
        dir.path(),
        "format_limit.xlsx",
        &[SheetSpec::new(
            "Data",
            row(1, &[number("A1", "1"), date_custom("B1", "46237")]),
        )],
        &styles(&"F".repeat(31), &format_code, 65_490),
    );

    let out = run_node(serde_json::json!({
        "path": path.to_str().unwrap(),
        "has_header": false
    }))
    .await
    .expect("a workbook within Excel's own limits is extracted");

    assert_eq!(out["content"]["Data"][0][0], serde_json::json!(1));
    assert_eq!(
        out["content"]["Data"][0][1],
        serde_json::json!("2026-08-03T00:00:00"),
        "cell formats must still type dates after the preflight"
    );
}
