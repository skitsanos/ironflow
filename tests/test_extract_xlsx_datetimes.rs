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

/// Serial numbers in a date-formatted cell, with the text each gave in the
/// 1900 and the 1904 date system before the conversion stopped going through
/// `chrono` (IF-231): both sides of the day Excel invents in 1900, rounding
/// at a second and at a day, both ends of the calendar, and values no
/// calendar holds.
const DATE_SERIALS: [(&str, Option<&str>, Option<&str>); 34] = [
    (
        "0",
        Some("1899-12-31T00:00:00"),
        Some("1904-01-01T00:00:00"),
    ),
    (
        "0.5",
        Some("1899-12-31T12:00:00"),
        Some("1904-01-01T12:00:00"),
    ),
    (
        "1",
        Some("1900-01-01T00:00:00"),
        Some("1904-01-02T00:00:00"),
    ),
    (
        "1.999999",
        Some("1900-01-01T23:59:59"),
        Some("1904-01-02T23:59:59"),
    ),
    (
        "1.9999999999",
        Some("1900-01-02T00:00:00"),
        Some("1904-01-03T00:00:00"),
    ),
    (
        "59",
        Some("1900-02-28T00:00:00"),
        Some("1904-02-29T00:00:00"),
    ),
    (
        "59.99999",
        Some("1900-02-28T23:59:59"),
        Some("1904-02-29T23:59:59"),
    ),
    (
        "60",
        Some("1900-02-28T00:00:00"),
        Some("1904-03-01T00:00:00"),
    ),
    (
        "60.5",
        Some("1900-02-28T12:00:00"),
        Some("1904-03-01T12:00:00"),
    ),
    (
        "61",
        Some("1900-03-01T00:00:00"),
        Some("1904-03-02T00:00:00"),
    ),
    (
        "366",
        Some("1900-12-31T00:00:00"),
        Some("1905-01-01T00:00:00"),
    ),
    (
        "367",
        Some("1901-01-01T00:00:00"),
        Some("1905-01-02T00:00:00"),
    ),
    (
        "25569",
        Some("1970-01-01T00:00:00"),
        Some("1974-01-02T00:00:00"),
    ),
    (
        "25569.000011574",
        Some("1970-01-01T00:00:01"),
        Some("1974-01-02T00:00:01"),
    ),
    (
        "36526.5",
        Some("2000-01-01T12:00:00"),
        Some("2004-01-02T12:00:00"),
    ),
    (
        "45000.123456",
        Some("2023-03-15T02:57:46"),
        Some("2027-03-16T02:57:46"),
    ),
    (
        "45000.9999942",
        Some("2023-03-15T23:59:59"),
        Some("2027-03-16T23:59:59"),
    ),
    (
        "45000.99999999999",
        Some("2023-03-16T00:00:00"),
        Some("2027-03-17T00:00:00"),
    ),
    (
        "2957003.5",
        Some("9995-12-30T12:00:00"),
        Some("9999-12-31T12:00:00"),
    ),
    (
        "2958465",
        Some("9999-12-31T00:00:00"),
        Some("+10004-01-01T00:00:00"),
    ),
    (
        "2958465.99999",
        Some("9999-12-31T23:59:59"),
        Some("+10004-01-01T23:59:59"),
    ),
    (
        "2958466",
        Some("+10000-01-01T00:00:00"),
        Some("+10004-01-02T00:00:00"),
    ),
    (
        "3000000",
        Some("+10113-09-19T00:00:00"),
        Some("+10117-09-20T00:00:00"),
    ),
    ("10000000000", None, None),
    ("1e300", None, None),
    ("inf", None, None),
    (
        "NaN",
        Some("1899-12-30T00:00:00"),
        Some("1899-12-30T00:00:00"),
    ),
    (
        "-0.5",
        Some("1899-12-30T12:00:00"),
        Some("1903-12-31T12:00:00"),
    ),
    (
        "-1",
        Some("1899-12-30T00:00:00"),
        Some("1903-12-31T00:00:00"),
    ),
    (
        "-36525",
        Some("1799-12-30T00:00:00"),
        Some("1804-01-01T00:00:00"),
    ),
    (
        "-693594",
        Some("0001-01-01T00:00:00"),
        Some("0005-01-02T00:00:00"),
    ),
    (
        "-693960.5",
        Some("-0001-12-31T12:00:00"),
        Some("0004-01-01T12:00:00"),
    ),
    (
        "-4346000",
        Some("-9999-01-20T00:00:00"),
        Some("-9995-01-21T00:00:00"),
    ),
    ("-10000000000", None, None),
];

/// Elapsed-time cells at the edge of what milliseconds in 64 bits hold.
const DURATION_SERIALS: [(&str, &str); 6] = [
    ("10000000000", "PT240000000000H"),
    ("-10000000000", "-PT240000000000H"),
    ("106751991167.3", "PT2562047788015H12M0.512S"),
    ("-106751991167.3", "-PT2562047788015H12M0.512S"),
    ("106751991168", "PT2562047788015H12M55.807S"),
    ("NaN", "PT0S"),
];

/// One cell per row, so a failure names its serial.
async fn single_cell(style: u8, serial: &str, workbook_pr: &str) -> serde_json::Value {
    first_row(&[cell("A1", style, serial)], workbook_pr).await[0].clone()
}

#[tokio::test]
async fn date_serials_keep_their_recorded_text() {
    for (serial, in_1900, in_1904) in DATE_SERIALS {
        let expected = |text: Option<&str>| text.map_or(serde_json::Value::Null, Into::into);
        assert_eq!(
            single_cell(3, serial, "").await,
            expected(in_1900),
            "{serial}, 1900"
        );
        let pr = r#"<workbookPr date1904="1"/>"#;
        assert_eq!(
            single_cell(3, serial, pr).await,
            expected(in_1904),
            "{serial}, 1904"
        );
    }
}

#[tokio::test]
async fn extreme_durations_keep_their_recorded_text() {
    for (serial, expected) in DURATION_SERIALS {
        assert_eq!(single_cell(1, serial, "").await, expected, "{serial}");
    }
}
