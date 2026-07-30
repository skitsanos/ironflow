#[path = "support/xlsx.rs"]
mod xlsx_support;

use xlsx_support::{SheetSpec, row, text, write_workbook};

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
