use std::collections::HashMap;

/// Parse `word/numbering.xml` into `numId -> is_numbered` mappings.
pub(in crate::nodes::extract) fn parse_numbering_defs(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> HashMap<String, bool> {
    let xml = match archive.by_name("word/numbering.xml") {
        Ok(entry) => match crate::util::bounded_read::read_to_string_capped(
            entry,
            crate::util::limits::max_zip_uncompressed_bytes(),
            "extract_word",
        ) {
            Ok(xml) => xml,
            Err(_) => return HashMap::new(),
        },
        Err(_) => return HashMap::new(),
    };

    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();
    let mut abstract_defs = HashMap::<String, bool>::new();
    let mut num_to_abstract = HashMap::<String, String>::new();
    let mut current_abstract_id = None;
    let mut current_num_id = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event) | Event::Empty(ref event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                match name.as_str() {
                    "w:abstractNum" => {
                        current_abstract_id = attribute(event, b"w:abstractNumId");
                    }
                    "w:numFmt" => {
                        if let (Some(id), Some(value)) =
                            (&current_abstract_id, attribute(event, b"w:val"))
                        {
                            abstract_defs.insert(id.clone(), value != "bullet" && value != "none");
                        }
                    }
                    "w:num" => current_num_id = attribute(event, b"w:numId"),
                    "w:abstractNumId" => {
                        if let (Some(num_id), Some(abstract_id)) =
                            (&current_num_id, attribute(event, b"w:val"))
                        {
                            num_to_abstract.insert(num_id.clone(), abstract_id);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if name == "w:abstractNum" {
                    current_abstract_id = None;
                } else if name == "w:num" {
                    current_num_id = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    num_to_abstract
        .into_iter()
        .filter_map(|(num_id, abstract_id)| {
            abstract_defs
                .get(&abstract_id)
                .copied()
                .map(|is_numbered| (num_id, is_numbered))
        })
        .collect()
}

fn attribute(event: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    event.attributes().flatten().find_map(|attr| {
        (attr.key.as_ref() == name).then(|| String::from_utf8_lossy(&attr.value).to_string())
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::parse_numbering_defs;

    #[test]
    fn resolves_ordered_and_bulleted_numbering_instances() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("numbering.docx");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "word/numbering.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer
            .write_all(
                br#"<w:numbering xmlns:w="urn:test">
                    <w:abstractNum w:abstractNumId="10">
                        <w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl>
                    </w:abstractNum>
                    <w:abstractNum w:abstractNumId="11">
                        <w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl>
                    </w:abstractNum>
                    <w:num w:numId="42"><w:abstractNumId w:val="10"/></w:num>
                    <w:num w:numId="43"><w:abstractNumId w:val="11"/></w:num>
                </w:numbering>"#,
            )
            .unwrap();
        writer.finish().unwrap();

        let file = std::fs::File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let numbering = parse_numbering_defs(&mut archive);

        assert_eq!(numbering.get("42"), Some(&true));
        assert_eq!(numbering.get("43"), Some(&false));
    }
}
