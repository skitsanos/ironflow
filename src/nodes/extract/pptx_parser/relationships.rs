/// Parse a slide relationship file into a map of relationship ID to target path.
pub(in crate::nodes::extract) fn parse_pptx_rels(
    xml: &str,
) -> std::collections::HashMap<String, String> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut map = std::collections::HashMap::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event) | Event::Empty(ref event)) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "Relationship" {
                    collect_relationship(event, &mut map);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    map
}

fn collect_relationship(
    event: &quick_xml::events::BytesStart<'_>,
    map: &mut std::collections::HashMap<String, String>,
) {
    let mut id = String::new();
    let mut target = String::new();
    for attr in event.attributes().flatten() {
        let value = String::from_utf8_lossy(&attr.value).to_string();
        match attr.key.as_ref() {
            b"Id" => id = value,
            b"Target" => target = value,
            _ => {}
        }
    }
    if !id.is_empty() && !target.is_empty() {
        map.insert(id, target);
    }
}

/// Resolve a relative relationship target against its source directory.
pub(in crate::nodes::extract) fn normalize_pptx_path(source_dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = source_dir
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    for segment in target.split('/') {
        if segment == ".." {
            parts.pop();
        } else if segment != "." && !segment.is_empty() {
            parts.push(segment);
        }
    }
    parts.join("/")
}

pub(in crate::nodes::extract) fn read_pptx_media(
    archive: &mut zip::ZipArchive<std::fs::File>,
    path: &str,
) -> Option<(Vec<u8>, String)> {
    let mut entry = archive.by_name(path).ok()?;
    let mut bytes = Vec::new();
    std::io::copy(&mut entry, &mut bytes).ok()?;
    Some((bytes, media_mime_type(path).to_string()))
}

fn media_mime_type(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}
