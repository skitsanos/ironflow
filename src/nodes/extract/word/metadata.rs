use std::collections::BTreeMap;

const KNOWN_TAGS: [&str; 10] = [
    "dc:title",
    "dc:creator",
    "dc:subject",
    "dc:description",
    "cp:keywords",
    "cp:lastModifiedBy",
    "dcterms:created",
    "dcterms:modified",
    "cp:revision",
    "cp:category",
];

pub(super) fn extract_docx_metadata(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> BTreeMap<String, String> {
    let Some(xml) = read_core_properties(archive) else {
        return BTreeMap::new();
    };
    parse_core_properties(&xml)
}

fn read_core_properties(archive: &mut zip::ZipArchive<std::fs::File>) -> Option<String> {
    let entry = archive.by_name("docProps/core.xml").ok()?;
    let xml = crate::util::bounded_read::read_to_string_capped(
        entry,
        crate::util::limits::max_zip_uncompressed_bytes(),
        "extract_word",
    )
    .ok()?;
    Some(xml)
}

fn parse_core_properties(xml: &str) -> BTreeMap<String, String> {
    use quick_xml::events::Event;

    let mut metadata = BTreeMap::new();
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut current_tag = String::new();
    let mut in_metadata = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if KNOWN_TAGS.contains(&name.as_str()) {
                    current_tag = name;
                    in_metadata = true;
                }
            }
            Ok(Event::Text(ref event)) if in_metadata => {
                let text = String::from_utf8_lossy(event.as_ref()).trim().to_string();
                if !text.is_empty() {
                    metadata.insert(key_for_tag(&current_tag).to_string(), text);
                }
            }
            Ok(Event::End(_)) => in_metadata = false,
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    metadata
}

fn key_for_tag(tag: &str) -> &'static str {
    match tag {
        "dc:title" => "title",
        "dc:creator" => "author",
        "dc:subject" => "subject",
        "dc:description" => "description",
        "cp:keywords" => "keywords",
        "cp:lastModifiedBy" => "last_modified_by",
        "dcterms:created" => "created",
        "dcterms:modified" => "modified",
        "cp:revision" => "revision",
        "cp:category" => "category",
        _ => "unknown",
    }
}
