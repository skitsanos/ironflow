use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RNS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PNS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const DOC: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

/// One worksheet: its name, and the `<row>` elements of its `<sheetData>`.
pub struct SheetSpec {
    pub name: String,
    pub rows_xml: String,
}

#[allow(dead_code)] // Each integration-test crate uses a different subset.
impl SheetSpec {
    pub fn new(name: &str, rows_xml: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            rows_xml: rows_xml.into(),
        }
    }
}

/// An inline-string cell. Inline strings avoid needing a shared-strings part.
#[allow(dead_code)]
pub fn text(reference: &str, value: &str) -> String {
    format!(r#"<c r="{reference}" t="inlineStr"><is><t>{value}</t></is></c>"#)
}

/// A plain numeric cell. calamine reads these as `Float`, even for integers.
#[allow(dead_code)]
pub fn number(reference: &str, value: &str) -> String {
    format!(r#"<c r="{reference}"><v>{value}</v></c>"#)
}

/// A boolean cell. `value` is "1" or "0".
#[allow(dead_code)]
pub fn boolean(reference: &str, value: &str) -> String {
    format!(r#"<c r="{reference}" t="b"><v>{value}</v></c>"#)
}

/// An error cell, e.g. `#DIV/0!`.
#[allow(dead_code)]
pub fn error(reference: &str, code: &str) -> String {
    format!(r#"<c r="{reference}" t="e"><v>{code}</v></c>"#)
}

/// A date cell using the custom `yyyy-mm-dd` format (style index 1).
#[allow(dead_code)]
pub fn date_custom(reference: &str, serial: &str) -> String {
    format!(r#"<c r="{reference}" s="1"><v>{serial}</v></c>"#)
}

/// A date cell using built-in number format id 14 (style index 2).
#[allow(dead_code)]
pub fn date_builtin(reference: &str, serial: &str) -> String {
    format!(r#"<c r="{reference}" s="2"><v>{serial}</v></c>"#)
}

/// Wrap cells into a `<row>`.
#[allow(dead_code)]
pub fn row(index: usize, cells: &[String]) -> String {
    format!(r#"<row r="{index}">{}</row>"#, cells.join(""))
}

/// Write a valid `.xlsx` containing `sheets`, in the order given.
#[allow(dead_code)]
pub fn write_workbook(dir: &Path, name: &str, sheets: &[SheetSpec]) -> PathBuf {
    let path = dir.join(name);
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    let add = |zip: &mut zip::ZipWriter<std::fs::File>, entry: &str, body: String| {
        zip.start_file(entry, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    };

    let overrides: String = (1..=sheets.len())
        .map(|n| {
            format!(
                r#"<Override PartName="/xl/worksheets/sheet{n}.xml" ContentType="{DOC}.worksheet+xml"/>"#
            )
        })
        .collect();
    add(
        &mut zip,
        "[Content_Types].xml",
        format!(
            r#"<Types xmlns="{CT}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="{DOC}.sheet.main+xml"/>{overrides}<Override PartName="/xl/styles.xml" ContentType="{DOC}.styles+xml"/></Types>"#
        ),
    );

    add(
        &mut zip,
        "_rels/.rels",
        format!(
            r#"<Relationships xmlns="{PNS}"><Relationship Id="rId1" Type="{RNS}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
        ),
    );

    let sheet_tags: String = sheets
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                r#"<sheet name="{}" sheetId="{}" r:id="rId{}"/>"#,
                s.name,
                i + 1,
                i + 1
            )
        })
        .collect();
    add(
        &mut zip,
        "xl/workbook.xml",
        format!(
            r#"<workbook xmlns="{NS}" xmlns:r="{RNS}"><sheets>{sheet_tags}</sheets></workbook>"#
        ),
    );

    let mut rels: String = sheets
        .iter()
        .enumerate()
        .map(|(i, _)| {
            format!(
                r#"<Relationship Id="rId{}" Type="{RNS}/worksheet" Target="worksheets/sheet{}.xml"/>"#,
                i + 1,
                i + 1
            )
        })
        .collect();
    rels.push_str(&format!(
        r#"<Relationship Id="rId{}" Type="{RNS}/styles" Target="styles.xml"/>"#,
        sheets.len() + 1
    ));
    add(
        &mut zip,
        "xl/_rels/workbook.xml.rels",
        format!(r#"<Relationships xmlns="{PNS}">{rels}</Relationships>"#),
    );

    // Style 0 = general, 1 = custom date code, 2 = built-in date format id 14.
    add(
        &mut zip,
        "xl/styles.xml",
        format!(
            r#"<styleSheet xmlns="{NS}"><numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy\-mm\-dd"/></numFmts><cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="164" applyNumberFormat="1"/><xf numFmtId="14" applyNumberFormat="1"/></cellXfs></styleSheet>"#
        ),
    );

    for (i, sheet) in sheets.iter().enumerate() {
        add(
            &mut zip,
            &format!("xl/worksheets/sheet{}.xml", i + 1),
            format!(
                r#"<worksheet xmlns="{NS}"><sheetData>{}</sheetData></worksheet>"#,
                sheet.rows_xml
            ),
        );
    }

    zip.finish().unwrap();
    path
}
