use std::collections::HashMap;

use super::PptxComment;

type Author = (Option<String>, Option<String>);

pub(in crate::nodes::extract) fn extract_pptx_comments(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> Vec<PptxComment> {
    let authors = read_authors(archive);
    let mut comment_names = comment_entry_names(archive);
    comment_names.sort_by_key(|name| comment_file_index(name));

    let mut comments = Vec::new();
    for name in comment_names {
        let slide_index = comment_file_index(&name);
        let Some(xml) = read_archive_string(archive, &name) else {
            continue;
        };
        comments.extend(parse_comments(&xml, slide_index, &authors));
    }
    comments
}

fn read_authors(archive: &mut zip::ZipArchive<std::fs::File>) -> HashMap<String, Author> {
    let Some(xml) = read_archive_string(archive, "ppt/commentAuthors.xml") else {
        return HashMap::new();
    };
    parse_authors(&xml)
}

fn read_archive_string(archive: &mut zip::ZipArchive<std::fs::File>, path: &str) -> Option<String> {
    let entry = archive.by_name(path).ok()?;
    let xml = crate::util::bounded_read::read_to_string_capped(
        entry,
        crate::util::limits::max_zip_uncompressed_bytes(),
        "extract_pptx",
    )
    .ok()?;
    (!xml.is_empty()).then_some(xml)
}

fn parse_authors(xml: &str) -> HashMap<String, Author> {
    use quick_xml::events::Event;

    let mut authors = HashMap::new();
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event) | Event::Empty(ref event)) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if raw.rsplit(':').next().unwrap_or(&raw) == "cmAuthor"
                    && let Some((id, author)) = parse_author(event)
                {
                    authors.insert(id, author);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    authors
}

fn parse_author(event: &quick_xml::events::BytesStart<'_>) -> Option<(String, Author)> {
    let mut id = String::new();
    let mut name = None;
    let mut initials = None;
    for attr in event.attributes().flatten() {
        let value = String::from_utf8_lossy(&attr.value).to_string();
        match attr.key.as_ref() {
            b"id" => id = value,
            b"name" => name = Some(value),
            b"initials" => initials = Some(value),
            _ => {}
        }
    }
    (!id.is_empty()).then_some((id, (name, initials)))
}

fn comment_entry_names(archive: &mut zip::ZipArchive<std::fs::File>) -> Vec<String> {
    (0..archive.len())
        .filter_map(|index| {
            let entry = archive.by_index(index).ok()?;
            let name = entry.name().to_string();
            (name.starts_with("ppt/comments/comment") && name.ends_with(".xml")).then_some(name)
        })
        .collect()
}

fn comment_file_index(name: &str) -> u32 {
    name.trim_start_matches("ppt/comments/comment")
        .trim_end_matches(".xml")
        .parse()
        .unwrap_or(0)
}

fn parse_comments(
    xml: &str,
    slide_index: u32,
    authors: &HashMap<String, Author>,
) -> Vec<PptxComment> {
    use quick_xml::events::Event;

    let mut comments = Vec::new();
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut current = None;
    let mut in_text = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event) | Event::Empty(ref event)) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                match raw.rsplit(':').next().unwrap_or(&raw) {
                    "cm" => current = Some(parse_comment(event, slide_index, authors)),
                    "text" => in_text = current.is_some(),
                    _ => {}
                }
            }
            Ok(Event::Text(ref event)) if in_text => {
                if let Some(comment) = current.as_mut() {
                    comment
                        .text
                        .push_str(&String::from_utf8_lossy(event.as_ref()));
                }
            }
            Ok(Event::End(ref event)) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                match raw.rsplit(':').next().unwrap_or(&raw) {
                    "text" => in_text = false,
                    "cm" => {
                        if let Some(comment) = current.take() {
                            comments.push(comment);
                        }
                    }
                    _ => {}
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

fn parse_comment(
    event: &quick_xml::events::BytesStart<'_>,
    slide_index: u32,
    authors: &HashMap<String, Author>,
) -> PptxComment {
    let mut comment = PptxComment {
        slide_index,
        ..Default::default()
    };
    for attr in event.attributes().flatten() {
        let value = String::from_utf8_lossy(&attr.value).to_string();
        match attr.key.as_ref() {
            b"authorId" => {
                if let Some((name, initials)) = authors.get(&value) {
                    comment.author = name.clone();
                    comment.initials = initials.clone();
                }
                comment.author_id = Some(value);
            }
            b"dt" => comment.date = Some(value),
            b"idx" => comment.idx = Some(value),
            _ => {}
        }
    }
    comment
}
