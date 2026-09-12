use std::io::{Cursor, Read, Write};

use super::{CappedRead, check, inspect, reference_output_bytes};

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

#[test]
fn utf8_text_cdata_and_references_are_accepted_and_counted_in_bytes() {
    let xml = "<x:sst xmlns:x=\"urn:test\" uniqueCount=\"1\"><x:si><x:t>caf\u{e9}\r\n<![CDATA[\u{6771}\u{4eac}\r\n]]>&#x1F600;&amp;</x:t></x:si></x:sst>";
    // Split every multi-byte character across reader fills.
    struct FragmentedReader<'a>(&'a [u8]);
    impl Read for FragmentedReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let len = buffer.len().min(1);
            self.0.read(&mut buffer[..len])
        }
    }
    inspect(FragmentedReader(xml.as_bytes()), 1, 1024, None).unwrap();
    for (reference, bytes) in [("#x1F600", 4), ("#233", 2), ("amp", 1), ("unknown", 9)] {
        assert_eq!(
            reference_output_bytes(&quick_xml::events::BytesRef::new(reference)).unwrap(),
            bytes
        );
    }
    assert!(reference_output_bytes(&quick_xml::events::BytesRef::new("#xZZ")).is_err());
}

#[test]
fn invalid_utf8_shared_string_content_is_rejected() {
    for xml in [
        b"<sst><si><t>\xff</t></si></sst>".as_slice(),
        b"<sst><si><t><![CDATA[\xff]]></t></si></sst>".as_slice(),
    ] {
        let error = inspect(xml, 1, 1024, None).unwrap_err().to_string();
        assert!(
            error.contains("invalid or oversized sharedStrings.xml"),
            "{error}"
        );
    }
}
