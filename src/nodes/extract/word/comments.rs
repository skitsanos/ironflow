use std::collections::{HashMap, HashSet};

#[derive(serde::Serialize, Default)]
pub(super) struct DocxComment {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initials: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    text: String,
    /// Source-document text between the comment range markers.
    #[serde(skip_serializing_if = "Option::is_none")]
    anchored_text: Option<String>,
}

pub(super) fn extract_docx_comments(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> Vec<DocxComment> {
    let Some(comments_xml) = read_archive_string(archive, "word/comments.xml") else {
        return Vec::new();
    };
    let mut comments = parse_comments(&comments_xml);
    if let Some(document_xml) = read_archive_string(archive, "word/document.xml") {
        let anchors = collect_anchors(&document_xml);
        for comment in &mut comments {
            if let Some(text) = anchors.get(&comment.id) {
                comment.anchored_text = Some(text.trim().to_string());
            }
        }
    }
    comments
}

fn read_archive_string(archive: &mut zip::ZipArchive<std::fs::File>, path: &str) -> Option<String> {
    let entry = archive.by_name(path).ok()?;
    let xml = crate::util::bounded_read::read_to_string_capped(
        entry,
        crate::util::limits::max_zip_uncompressed_bytes(),
        "extract_word",
    )
    .ok()?;
    Some(xml)
}

fn parse_comments(xml: &str) -> Vec<DocxComment> {
    use quick_xml::events::Event;

    let mut comments = Vec::new();
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut current = None;
    let mut in_text = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event) | Event::Empty(ref event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if name == "w:comment" {
                    current = Some(parse_comment(event));
                } else if name == "w:t" {
                    in_text = current.is_some();
                }
            }
            Ok(Event::Text(ref event)) if in_text => {
                if let Some(comment) = current.as_mut() {
                    if !comment.text.is_empty() {
                        comment.text.push(' ');
                    }
                    comment
                        .text
                        .push_str(&String::from_utf8_lossy(event.as_ref()));
                }
            }
            Ok(Event::End(ref event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if name == "w:t" {
                    in_text = false;
                } else if name == "w:comment"
                    && let Some(comment) = current.take()
                {
                    comments.push(comment);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    comments
}

fn parse_comment(event: &quick_xml::events::BytesStart<'_>) -> DocxComment {
    let mut comment = DocxComment::default();
    for attr in event.attributes().flatten() {
        let value = String::from_utf8_lossy(&attr.value).to_string();
        match attr.key.as_ref() {
            b"w:id" => comment.id = value,
            b"w:author" => comment.author = Some(value),
            b"w:initials" => comment.initials = Some(value),
            b"w:date" => comment.date = Some(value),
            _ => {}
        }
    }
    comment
}

fn collect_anchors(xml: &str) -> HashMap<String, String> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut open = HashSet::new();
    let mut anchors: HashMap<String, String> = HashMap::new();
    let mut in_text = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event) | Event::Empty(ref event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                match name.as_str() {
                    "w:commentRangeStart" => update_open_ranges(event, &mut open, true),
                    "w:commentRangeEnd" => update_open_ranges(event, &mut open, false),
                    "w:t" => in_text = true,
                    _ => {}
                }
            }
            Ok(Event::Text(ref event)) if in_text && !open.is_empty() => {
                let text = String::from_utf8_lossy(event.as_ref());
                for id in &open {
                    let anchor = anchors.entry(id.clone()).or_default();
                    if !anchor.is_empty() {
                        anchor.push(' ');
                    }
                    anchor.push_str(&text);
                }
            }
            Ok(Event::End(ref event)) => {
                if event.name().as_ref() == b"w:t" {
                    in_text = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    anchors
}

fn update_open_ranges(
    event: &quick_xml::events::BytesStart<'_>,
    open: &mut HashSet<String>,
    insert: bool,
) {
    for attr in event.attributes().flatten() {
        if attr.key.as_ref() == b"w:id" {
            let id = String::from_utf8_lossy(&attr.value).to_string();
            if insert {
                open.insert(id);
            } else {
                open.remove(&id);
            }
        }
    }
}
