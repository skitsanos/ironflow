use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;

use super::inspect;
use crate::util::execution::ExecutionControl;

/// Admit the archive with the raw preflight, as `workbook::extract` does,
/// then run the check under test against the admitted directory.
fn check<R: Read + Seek>(
    mut reader: R,
    path: &Path,
    max_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> anyhow::Result<()> {
    let admitted = super::super::archive_preflight::check_xlsx(
        &mut reader,
        path,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        execution,
    )?;
    super::check(reader, path, admitted, max_bytes, execution)
}

fn inspect_xml(xml: &str, max_bytes: u64) -> anyhow::Result<()> {
    inspect(Cursor::new(xml.as_bytes()), max_bytes, None)
}

/// An archive holding `parts` as `(name, body)`, compressed.
fn archive(parts: &[(&str, &str)]) -> Cursor<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in parts {
        zip.start_file(*name, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    Cursor::new(zip.finish().unwrap().into_inner())
}

fn check_archive(parts: &[(&str, &str)], max_bytes: u64) -> anyhow::Result<()> {
    check(
        archive(parts),
        std::path::Path::new("book.xlsx"),
        max_bytes,
        None,
    )
}

/// One font with a `name_bytes` name, referenced by `formats` cell formats.
fn repeated_name(name_bytes: usize, formats: usize) -> String {
    format!(
        r#"<styleSheet><fonts><font><name val="{}"/></font></fonts><cellXfs>{}</cellXfs></styleSheet>"#,
        "n".repeat(name_bytes),
        r#"<xf fontId="0"/>"#.repeat(formats)
    )
}

#[test]
fn a_font_name_is_charged_once_for_every_cell_format() {
    // 4 KiB of XML: one 2 KiB name and 100 formats. The part fits a 64 KiB
    // limit many times over; the 100 private copies calamine makes do not.
    let xml = repeated_name(2 * 1024, 100);
    assert!(xml.len() < 5 * 1024);

    let error = inspect_xml(&xml, 64 * 1024).unwrap_err().to_string();

    // Refused while streaming, as soon as the copies would cross the limit.
    assert!(error.contains("styles.xml holds"), "{error}");
    assert!(error.contains("style records"), "{error}");
    assert!(
        error.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES (65536)"),
        "{error}"
    );
}

#[test]
fn a_format_code_is_charged_once_for_every_cell_format() {
    let xml = format!(
        r#"<styleSheet><numFmts><numFmt numFmtId="164" formatCode="{}"/></numFmts><cellXfs>{}</cellXfs></styleSheet>"#,
        "0".repeat(2 * 1024),
        r#"<xf numFmtId="164"/>"#.repeat(100)
    );

    let error = inspect_xml(&xml, 64 * 1024).unwrap_err().to_string();

    assert!(error.contains("style records"), "{error}");
}

#[test]
fn the_same_strings_with_few_formats_are_accepted() {
    // Two formats copy the 2 KiB name twice: about 5 KiB, inside the limit.
    inspect_xml(&repeated_name(2 * 1024, 2), 64 * 1024).unwrap();
}

#[test]
fn records_without_any_string_still_cost_their_fixed_size() {
    let xml = format!(
        "<styleSheet><cellXfs>{}</cellXfs></styleSheet>",
        "<xf/>".repeat(1_000)
    );
    assert!(xml.len() < 6 * 1024);

    // 1,000 records of 320 bytes need 320,000 bytes.
    let error = inspect_xml(&xml, 319_999).unwrap_err().to_string();
    assert!(error.contains("style records"), "{error}");
    inspect_xml(&xml, 320_000).unwrap();
}

#[test]
fn fonts_fills_and_borders_are_charged_as_records() {
    let xml = format!(
        "<styleSheet><fonts>{}</fonts><fills>{}</fills><borders>{}</borders></styleSheet>",
        "<font/>".repeat(10),
        "<fill/>".repeat(10),
        "<border/>".repeat(10)
    );

    let error = inspect_xml(&xml, 30 * 320 - 1).unwrap_err().to_string();
    assert!(error.contains("30 style records"), "{error}");
    inspect_xml(&xml, 30 * 320).unwrap();
}

#[test]
fn prefixed_elements_are_counted_like_calamine_matches_them() {
    // calamine compares local names, so a namespace prefix hides nothing.
    let xml = format!(
        r#"<x:styleSheet xmlns:x="urn:x"><x:fonts><x:font><x:name x:val="{}"/></x:font></x:fonts><x:cellXfs>{}</x:cellXfs></x:styleSheet>"#,
        "n".repeat(2 * 1024),
        r#"<x:xf fontId="0"/>"#.repeat(100)
    );

    let error = inspect_xml(&xml, 64 * 1024).unwrap_err().to_string();

    assert!(error.contains("style records"), "{error}");
}

#[test]
fn formats_and_strings_are_counted_wherever_they_appear() {
    // Formats outside `cellXfs`, and a long string declared after them, are
    // charged all the same: the bound never depends on document order.
    let xml = format!(
        r#"<styleSheet><cellStyleXfs>{}</cellStyleXfs><dxfs><dxf><font><name val="{}"/></font></dxf></dxfs></styleSheet>"#,
        "<xf/>".repeat(100),
        "n".repeat(2 * 1024)
    );

    let error = inspect_xml(&xml, 64 * 1024).unwrap_err().to_string();

    assert!(error.contains("101 style records"), "{error}");
}

#[test]
fn an_ordinary_style_sheet_is_accepted() {
    let xml = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy\-mm\-dd"/></numFmts><fonts count="1"><font><sz val="11"/><color theme="1"/><name val="Calibri"/><family val="2"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellXfs count="2"><xf numFmtId="0" fontId="0"/><xf numFmtId="164" fontId="0" applyNumberFormat="1"><alignment horizontal="center"/></xf></cellXfs></styleSheet>"#;

    inspect_xml(xml, 4 * 1024).unwrap();
}

#[test]
fn a_part_longer_than_the_limit_is_refused() {
    let xml = format!("<styleSheet>{}</styleSheet>", " ".repeat(200));

    let error = inspect_xml(&xml, 100).unwrap_err().to_string();

    assert!(error.contains("actual uncompressed bytes"), "{error}");
    assert!(
        error.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES (100)"),
        "{error}"
    );
}

#[test]
fn malformed_markup_fails_closed() {
    let error = inspect_xml("<styleSheet><cellXfs><xf", 4 * 1024)
        .unwrap_err()
        .to_string();

    assert!(error.contains("invalid styles.xml"), "{error}");
}

#[test]
fn the_part_is_found_whatever_its_case_separator_or_directory() {
    let amplifier = repeated_name(2 * 1024, 100);
    for name in [
        "xl/styles.xml",
        "XL/Styles.XML",
        "xl\\styles.xml",
        "styles.xml",
        "custom/book/styles.xml",
    ] {
        let error = check_archive(&[(name, &amplifier)], 64 * 1024)
            .expect_err(name)
            .to_string();
        assert!(error.contains("style records"), "{name}: {error}");
    }
}

#[test]
fn every_candidate_part_is_checked() {
    // Which directory calamine reads depends on the package relationships;
    // a benign decoy must not shadow the part that is actually opened.
    let benign = repeated_name(7, 2);
    let amplifier = repeated_name(2 * 1024, 100);

    let error = check_archive(
        &[("xl/styles.xml", &benign), ("alt/styles.xml", &amplifier)],
        64 * 1024,
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("style records"), "{error}");
}

#[test]
fn parts_that_only_resemble_the_name_are_ignored() {
    let amplifier = repeated_name(2 * 1024, 100);

    check_archive(
        &[
            ("xl/mystyles.xml", &amplifier),
            ("xl/styles.xml.bak", &amplifier),
            ("xl/charts/style1.xml", &amplifier),
        ],
        64 * 1024,
    )
    .unwrap();
}

#[test]
fn a_workbook_without_a_style_part_is_accepted() {
    check_archive(&[("xl/workbook.xml", "<workbook/>")], 1).unwrap();
}

#[test]
fn an_open_that_differs_from_the_admitted_directory_is_refused() {
    let mut other = archive(&[("xl/a-longer-part-name.xml", "<x/>")]);
    let admitted = super::super::archive_preflight::check_xlsx(
        &mut other,
        Path::new("other.xlsx"),
        u64::MAX,
        u64::MAX,
        u64::MAX,
        None,
    )
    .unwrap();
    let book = archive(&[("xl/styles.xml", "<styleSheet/>")]);
    let error = super::check(book, Path::new("book.xlsx"), admitted, 1024, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("different central directory"), "{error}");
}
