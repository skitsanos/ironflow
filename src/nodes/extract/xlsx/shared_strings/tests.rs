use std::io::{Cursor, Read, Write};

use super::{CappedRead, check};

fn write_table(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("strings.xlsx");
    let file = std::fs::File::create(&path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file(
            "xl/sharedStrings.xml",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
    archive.write_all(body.as_bytes()).unwrap();
    archive.finish().unwrap();
    (directory, path)
}

#[test]
fn exaggerated_unique_count_is_rejected_before_calamine_can_reserve() {
    let (_directory, path) =
        write_table(r#"<sst uniqueCount="18446744073709551615"><si><t>x</t></si></sst>"#);

    let file = std::fs::File::open(&path).unwrap();
    let error = check(file, &path, 100, 1024 * 1024, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("uniqueCount"), "{error}");
    assert!(error.contains("IRONFLOW_MAX_XLSX_CELLS"), "{error}");
}

#[test]
fn actual_string_count_is_bounded_even_when_unique_count_lies() {
    let (_directory, path) =
        write_table(r#"<sst uniqueCount="1"><si><t>a</t></si><si><t>b</t></si></sst>"#);

    let file = std::fs::File::open(&path).unwrap();
    let error = check(file, &path, 1, 1024 * 1024, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("actual shared-string count"), "{error}");
}

#[test]
fn actual_streaming_bytes_are_capped_independently_of_metadata() {
    let mut reader = CappedRead::new(Cursor::new(b"12345"), 4);
    let mut output = Vec::new();
    let error = reader.read_to_end(&mut output).unwrap_err().to_string();

    assert_eq!(output, b"1234");
    assert!(
        error.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES (4)"),
        "{error}"
    );
}

#[test]
fn a_small_well_formed_table_is_accepted() {
    let (_directory, path) =
        write_table(r#"<sst uniqueCount="2"><si><t>a&amp;b</t></si><si><t>c</t></si></sst>"#);

    let file = std::fs::File::open(&path).unwrap();
    check(file, &path, 2, 1024, None).unwrap();
}
