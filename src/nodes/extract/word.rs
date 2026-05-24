use std::collections::BTreeMap;
use std::io::Read;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::common::{get_path, validate_word_format};
use super::docx_parser::{parse_docx_blocks, parse_numbering_defs, parse_theme_colors};
use super::word_format::{blocks_to_json, blocks_to_markdown, blocks_to_text};

pub struct ExtractWordNode;

#[async_trait]
impl Node for ExtractWordNode {
    fn node_type(&self) -> &str {
        "extract_word"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from a Word (.docx) document"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_word")?;
        let format = validate_word_format(config, "extract_word")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("content");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());
        let comments_key = config.get("comments_key").and_then(|v| v.as_str());

        let file = std::fs::File::open(&path)
            .map_err(|e| anyhow::anyhow!("Failed to open '{}': {}", path, e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| anyhow::anyhow!("Failed to read DOCX archive '{}': {}", path, e))?;

        // Extract content from word/document.xml
        let content = extract_docx_content(&mut archive, format)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), content);

        // Extract metadata only when requested
        if let Some(meta_key) = metadata_key {
            let metadata = extract_docx_metadata(&mut archive);
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        };

        // Extract comments only when requested
        if let Some(c_key) = comments_key {
            let comments = extract_docx_comments(&mut archive);
            output.insert(c_key.to_string(), serde_json::to_value(comments)?);
        }
        Ok(output)
    }
}

pub(super) fn extract_docx_metadata(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> BTreeMap<String, String> {
    let mut meta = BTreeMap::new();

    let xml = match archive.by_name("docProps/core.xml") {
        Ok(mut entry) => {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                buf
            } else {
                return meta;
            }
        }
        Err(_) => return meta,
    };

    // Parse Dublin Core metadata from core.xml
    let reader = quick_xml::Reader::from_str(&xml);
    let mut current_tag = String::new();
    let mut in_meta = false;

    let known_tags = [
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

    use quick_xml::events::Event;
    let mut reader = reader;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if known_tags.iter().any(|t| *t == name) {
                    current_tag = name;
                    in_meta = true;
                }
            }
            Ok(Event::Text(ref e)) if in_meta => {
                let text = String::from_utf8_lossy(e.as_ref()).trim().to_string();
                if !text.is_empty() {
                    meta.insert(key_for_tag(&current_tag).to_string(), text);
                }
            }
            Ok(Event::End(_)) => {
                in_meta = false;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    meta
}

fn extract_docx_content(
    archive: &mut zip::ZipArchive<std::fs::File>,
    format: &str,
) -> Result<serde_json::Value> {
    let xml = {
        let mut entry = archive
            .by_name("word/document.xml")
            .map_err(|e| anyhow::anyhow!("Missing word/document.xml: {}", e))?;
        let mut buf = String::new();
        entry.read_to_string(&mut buf)?;
        buf
    };

    // Parse numbering definitions if available (for list detection)
    let numbering_abstract_ids = parse_numbering_defs(archive);
    // Parse theme colors so themeColor references resolve to hex
    let theme_colors = parse_theme_colors(archive);

    let blocks = parse_docx_blocks(&xml, &numbering_abstract_ids, &theme_colors);

    match format {
        "markdown" => Ok(serde_json::Value::String(blocks_to_markdown(&blocks))),
        "json" => Ok(blocks_to_json(&blocks)),
        _ => Ok(serde_json::Value::String(blocks_to_text(&blocks))),
    }
}

/// Single comment record extracted from a .docx.
#[derive(serde::Serialize, Default)]
struct DocxComment {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initials: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    text: String,
    /// Source-document text the comment is anchored to (between commentRangeStart/End).
    #[serde(skip_serializing_if = "Option::is_none")]
    anchored_text: Option<String>,
}

/// Parse word/comments.xml + walk word/document.xml for anchor ranges. Returns
/// the merged list of comments. Returns an empty vec if no comments part exists.
fn extract_docx_comments(archive: &mut zip::ZipArchive<std::fs::File>) -> Vec<DocxComment> {
    let comments_xml = match archive.by_name("word/comments.xml") {
        Ok(mut entry) => {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                buf
            } else {
                return Vec::new();
            }
        }
        Err(_) => return Vec::new(),
    };

    // Parse the comments part first — id → DocxComment (minus anchored_text).
    let mut comments: Vec<DocxComment> = parse_docx_comments_xml(&comments_xml);

    // Walk word/document.xml for commentRangeStart/End markers to grab anchored_text.
    let doc_xml = match archive.by_name("word/document.xml") {
        Ok(mut entry) => {
            let mut buf = String::new();
            entry.read_to_string(&mut buf).ok();
            buf
        }
        Err(_) => String::new(),
    };
    if !doc_xml.is_empty() {
        let anchors = collect_comment_anchors(&doc_xml);
        for c in &mut comments {
            if let Some(text) = anchors.get(&c.id) {
                c.anchored_text = Some(text.trim().to_string());
            }
        }
    }

    comments
}

fn parse_docx_comments_xml(xml: &str) -> Vec<DocxComment> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();

    let mut comments: Vec<DocxComment> = Vec::new();
    let mut current: Option<DocxComment> = None;
    let mut in_text = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "w:comment" {
                    let mut c = DocxComment::default();
                    for attr in e.attributes().flatten() {
                        let v = String::from_utf8_lossy(&attr.value).to_string();
                        match attr.key.as_ref() {
                            b"w:id" => c.id = v,
                            b"w:author" => c.author = Some(v),
                            b"w:initials" => c.initials = Some(v),
                            b"w:date" => c.date = Some(v),
                            _ => {}
                        }
                    }
                    current = Some(c);
                } else if name == "w:t" {
                    in_text = current.is_some();
                }
            }
            Ok(Event::Text(ref e)) if in_text => {
                if let Some(c) = current.as_mut() {
                    let t = String::from_utf8_lossy(e.as_ref());
                    if !c.text.is_empty() {
                        c.text.push(' ');
                    }
                    c.text.push_str(&t);
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "w:t" {
                    in_text = false;
                } else if name == "w:comment"
                    && let Some(c) = current.take()
                {
                    comments.push(c);
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

/// Walk document.xml and collect, for each comment id, the run text between
/// w:commentRangeStart and w:commentRangeEnd (the "anchored" text).
fn collect_comment_anchors(xml: &str) -> std::collections::HashMap<String, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();

    // Set of ids whose range is currently open.
    let mut open: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut in_text = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "w:commentRangeStart" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"w:id" {
                            open.insert(String::from_utf8_lossy(&attr.value).to_string());
                        }
                    }
                } else if name == "w:commentRangeEnd" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"w:id" {
                            open.remove(&*String::from_utf8_lossy(&attr.value));
                        }
                    }
                } else if name == "w:t" {
                    in_text = true;
                }
            }
            Ok(Event::Text(ref e)) if in_text && !open.is_empty() => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();
                for id in &open {
                    let slot = out.entry(id.clone()).or_default();
                    if !slot.is_empty() {
                        slot.push(' ');
                    }
                    slot.push_str(&text);
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "w:t" {
                    in_text = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out
}
