//! `extract_xlsx` date, time and elapsed-time cells (IF-227).
//!
//! A date-formatted cell is an ISO-8601 date-time; an elapsed-time format
//! (`[h]:mm:ss`) is an ISO-8601 duration, never a date.

#[path = "support/xlsx.rs"]
mod xlsx_support;

use xlsx_support::{SheetSpec, row, write_workbook_with_properties};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

/// Style 1 = built-in 46 (`[h]:mm:ss`), 2 = built-in 20 (`h:mm`), 3 = built-in
/// 22 (date and time), 4 = custom `[mm]:ss.000` (elapsed minutes).
fn styles() -> String {
    format!(
        r#"<styleSheet xmlns="{NS}"><numFmts count="1"><numFmt numFmtId="164" formatCode="[mm]:ss.000"/></numFmts><cellXfs count="5"><xf numFmtId="0"/><xf numFmtId="46" applyNumberFormat="1"/><xf numFmtId="20" applyNumberFormat="1"/><xf numFmtId="22" applyNumberFormat="1"/><xf numFmtId="164" applyNumberFormat="1"/></cellXfs></styleSheet>"#
    )
}

fn cell(reference: &str, style: u8, value: &str) -> String {
    format!(r#"<c r="{reference}" s="{style}"><v>{value}</v></c>"#)
}

async fn first_row(cells: &[String], workbook_pr: &str) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    let path = write_workbook_with_properties(
        dir.path(),
        "dates.xlsx",
        &[SheetSpec::new("Data", row(1, cells))],
        &styles(),
        workbook_pr,
    );
    let node = NodeRegistry::with_builtins().get("extract_xlsx").unwrap();
    let ctx = node
        .execute(
            &serde_json::json!({ "path": path.to_str().unwrap(), "has_header": false }),
            &Context::new(),
        )
        .await
        .unwrap();
    ctx["content"]["Data"][0].clone()
}

#[tokio::test]
async fn elapsed_time_formats_return_iso_durations_not_dates() {
    let row = first_row(
        &[
            cell("A1", 1, "1.5625"),                 // 37:30:00, over a day
            cell("B1", 1, "0"),                      // zero
            cell("C1", 1, "-0.0625"),                // negative: -1:30:00
            cell("D1", 4, "0.0010416666666666667"),  // 1 min 30.000 s
            cell("E1", 4, "0.00000578703703703704"), // 0.5 s
            cell("F1", 1, "0.5"),                    // 12:00:00 is still a duration
            cell("G1", 1, "100.000011574074"),       // 2400 h and 1 s
        ],
        "",
    )
    .await;
    assert_eq!(row[0], "PT37H30M");
    assert_eq!(row[1], "PT0S");
    assert_eq!(row[2], "-PT1H30M");
    assert_eq!(row[3], "PT1M30S");
    assert_eq!(row[4], "PT0.5S");
    assert_eq!(row[5], "PT12H");
    assert_eq!(row[6], "PT2400H1S");
}

#[tokio::test]
async fn time_only_cells_keep_excels_day_zero() {
    let row = first_row(&[cell("A1", 2, "0.6041666666666666")], "").await;
    assert_eq!(row[0], "1899-12-31T14:30:00");
}

#[tokio::test]
async fn the_1904_date_system_shifts_dates_and_leaves_durations_alone() {
    let pr = r#"<workbookPr date1904="1"/>"#;
    let row = first_row(
        &[
            cell("A1", 3, "0"),                  // day zero of the 1904 system
            cell("B1", 3, "44634.6041666667"),   // 2026-03-15 14:30 + 1462 days
            cell("C1", 1, "1.5625"),             // durations ignore the epoch
            cell("D1", 2, "0.6041666666666666"), // time only
        ],
        pr,
    )
    .await;
    assert_eq!(row[0], "1904-01-01T00:00:00");
    assert_eq!(row[1], "2026-03-15T14:30:00");
    assert_eq!(row[2], "PT37H30M");
    assert_eq!(row[3], "1904-01-01T14:30:00", "day zero of the 1904 system");
}
