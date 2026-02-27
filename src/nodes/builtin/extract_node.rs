use std::collections::BTreeMap;
use std::io::Read;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

/// Validate the `format` parameter — must be "text" or "markdown".
fn validate_format<'a>(config: &'a serde_json::Value, node_name: &str) -> Result<&'a str> {
    let format = config
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    match format {
        "text" | "markdown" => Ok(format),
        other => anyhow::bail!(
            "{}: unsupported format '{}'. Must be 'text' or 'markdown'.",
            node_name,
            other
        ),
    }
}

/// Get the file path from config — either `path` (literal) or `source_key` (from context).
fn get_path(config: &serde_json::Value, ctx: &Context, node_name: &str) -> Result<String> {
    let has_path = config.get("path").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_path && has_source_key {
        anyhow::bail!(
            "{} accepts either 'path' or 'source_key', not both",
            node_name
        );
    }

    if let Some(path_str) = config.get("path").and_then(|v| v.as_str()) {
        Ok(interpolate_ctx(path_str, ctx))
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        match val {
            serde_json::Value::String(s) => Ok(s.clone()),
            _ => anyhow::bail!("Context key '{}' must be a string (file path)", source_key),
        }
    } else {
        anyhow::bail!("{} requires either 'path' or 'source_key'", node_name)
    }
}

// ---------------------------------------------------------------------------
// extract_word
// ---------------------------------------------------------------------------

pub struct ExtractWordNode;

#[async_trait]
impl Node for ExtractWordNode {
    fn node_type(&self) -> &str {
        "extract_word"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from a Word (.docx) document"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = get_path(config, &ctx, "extract_word")?;
        let format = validate_format(config, "extract_word")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("content");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());

        let file = std::fs::File::open(&path)
            .map_err(|e| anyhow::anyhow!("Failed to open '{}': {}", path, e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| anyhow::anyhow!("Failed to read DOCX archive '{}': {}", path, e))?;

        // Extract content from word/document.xml
        let content = extract_docx_content(&mut archive, format)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(content));

        // Extract metadata only when requested
        if let Some(meta_key) = metadata_key {
            let metadata = extract_docx_metadata(&mut archive);
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        };
        Ok(output)
    }
}

fn extract_docx_metadata(archive: &mut zip::ZipArchive<std::fs::File>) -> BTreeMap<String, String> {
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
            Ok(Event::Text(ref e)) => {
                if in_meta {
                    let text = String::from_utf8_lossy(e.as_ref()).trim().to_string();
                    if !text.is_empty() {
                        meta.insert(key_for_tag(&current_tag).to_string(), text);
                    }
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

/// Structured representation of a DOCX paragraph for markdown conversion.
struct DocxParagraph {
    style: Option<String>,
    runs: Vec<DocxRun>,
    is_list_item: bool,
    list_level: u32,
    is_numbered: bool,
}

struct DocxRun {
    text: String,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
}

fn extract_docx_content(
    archive: &mut zip::ZipArchive<std::fs::File>,
    format: &str,
) -> Result<String> {
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

    let paragraphs = parse_docx_paragraphs(&xml, &numbering_abstract_ids);

    match format {
        "markdown" => Ok(paragraphs_to_markdown(&paragraphs)),
        _ => Ok(paragraphs_to_text(&paragraphs)),
    }
}

/// Parse numbering.xml to identify which numId values are numbered vs bulleted.
fn parse_numbering_defs(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> std::collections::HashMap<String, bool> {
    // Maps numId -> is_numbered (true = ordered, false = bullet)
    let mut result = std::collections::HashMap::new();

    let xml = match archive.by_name("word/numbering.xml") {
        Ok(mut entry) => {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                buf
            } else {
                return result;
            }
        }
        Err(_) => return result,
    };

    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();

    // Track abstractNum definitions: abstractNumId -> is_numbered
    let mut abstract_defs: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();
    let mut current_abstract_id: Option<String> = None;

    // Track num -> abstractNumId mapping
    let mut num_to_abstract: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut current_num_id: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "w:abstractNum" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:abstractNumId" {
                                current_abstract_id =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "w:lvl" => {}
                    "w:numFmt" => {
                        if let Some(ref id) = current_abstract_id {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"w:val" {
                                    let val = String::from_utf8_lossy(&attr.value).to_string();
                                    let is_numbered = val != "bullet" && val != "none";
                                    abstract_defs.insert(id.clone(), is_numbered);
                                }
                            }
                        }
                    }
                    "w:num" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:numId" {
                                current_num_id =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "w:abstractNumId" => {
                        if let Some(ref num_id) = current_num_id {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"w:val" {
                                    num_to_abstract.insert(
                                        num_id.clone(),
                                        String::from_utf8_lossy(&attr.value).to_string(),
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
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

    // Resolve: numId -> is_numbered
    for (num_id, abstract_id) in &num_to_abstract {
        if let Some(&is_numbered) = abstract_defs.get(abstract_id) {
            result.insert(num_id.clone(), is_numbered);
        }
    }

    result
}

fn parse_docx_paragraphs(
    xml: &str,
    numbering_defs: &std::collections::HashMap<String, bool>,
) -> Vec<DocxParagraph> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut paragraphs = Vec::new();

    let mut in_paragraph = false;
    let mut in_run = false;
    let mut in_run_props = false;
    let mut in_para_props = false;

    let mut current_para = DocxParagraph {
        style: None,
        runs: Vec::new(),
        is_list_item: false,
        list_level: 0,
        is_numbered: false,
    };
    let mut current_run = DocxRun {
        text: String::new(),
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match name.as_str() {
                    "w:p" => {
                        in_paragraph = true;
                        current_para = DocxParagraph {
                            style: None,
                            runs: Vec::new(),
                            is_list_item: false,
                            list_level: 0,
                            is_numbered: false,
                        };
                    }
                    "w:pPr" => {
                        if in_paragraph {
                            in_para_props = true;
                        }
                    }
                    "w:pStyle" => {
                        if in_para_props {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"w:val" {
                                    current_para.style =
                                        Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                        }
                    }
                    "w:numPr" => {
                        if in_para_props {
                            current_para.is_list_item = true;
                        }
                    }
                    "w:ilvl" => {
                        if in_para_props {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"w:val"
                                    && let Ok(level) =
                                        String::from_utf8_lossy(&attr.value).parse::<u32>()
                                {
                                    current_para.list_level = level;
                                }
                            }
                        }
                    }
                    "w:numId" => {
                        if in_para_props {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"w:val" {
                                    let num_id = String::from_utf8_lossy(&attr.value).to_string();
                                    current_para.is_numbered =
                                        numbering_defs.get(&num_id).copied().unwrap_or(false);
                                }
                            }
                        }
                    }
                    "w:r" => {
                        if in_paragraph {
                            in_run = true;
                            current_run = DocxRun {
                                text: String::new(),
                                bold: false,
                                italic: false,
                                underline: false,
                                strikethrough: false,
                            };
                        }
                    }
                    "w:rPr" => {
                        if in_run {
                            in_run_props = true;
                        }
                    }
                    "w:b" => {
                        if in_run_props {
                            current_run.bold = true;
                        }
                    }
                    "w:i" => {
                        if in_run_props {
                            current_run.italic = true;
                        }
                    }
                    "w:u" => {
                        if in_run_props {
                            current_run.underline = true;
                        }
                    }
                    "w:strike" => {
                        if in_run_props {
                            current_run.strikethrough = true;
                        }
                    }
                    "w:tab" => {
                        if in_run {
                            current_run.text.push('\t');
                        }
                    }
                    "w:br" => {
                        if in_run {
                            current_run.text.push('\n');
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_run {
                    let text = String::from_utf8_lossy(e.as_ref());
                    current_run.text.push_str(&text);
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "w:p" => {
                        in_paragraph = false;
                        paragraphs.push(current_para);
                        current_para = DocxParagraph {
                            style: None,
                            runs: Vec::new(),
                            is_list_item: false,
                            list_level: 0,
                            is_numbered: false,
                        };
                    }
                    "w:r" => {
                        in_run = false;
                        if !current_run.text.is_empty() {
                            current_para.runs.push(current_run);
                            current_run = DocxRun {
                                text: String::new(),
                                bold: false,
                                italic: false,
                                underline: false,
                                strikethrough: false,
                            };
                        }
                    }
                    "w:rPr" => in_run_props = false,
                    "w:pPr" => in_para_props = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    paragraphs
}

fn paragraphs_to_text(paragraphs: &[DocxParagraph]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for para in paragraphs {
        let text: String = para.runs.iter().map(|r| r.text.as_str()).collect();
        if !text.is_empty() || !para.runs.is_empty() {
            lines.push(text);
        }
    }
    lines.join("\n")
}

fn paragraphs_to_markdown(paragraphs: &[DocxParagraph]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut numbered_counters: std::collections::HashMap<u32, u32> =
        std::collections::HashMap::new();

    for para in paragraphs {
        let text: String = para.runs.iter().map(format_run_markdown).collect();

        if text.is_empty() && para.runs.is_empty() {
            continue;
        }

        // Heading styles
        if let Some(ref style) = para.style {
            let heading_level = match style.as_str() {
                "Heading1" | "heading1" | "Title" => Some(1),
                "Heading2" | "heading2" | "Subtitle" => Some(2),
                "Heading3" | "heading3" => Some(3),
                "Heading4" | "heading4" => Some(4),
                "Heading5" | "heading5" => Some(5),
                "Heading6" | "heading6" => Some(6),
                _ => None,
            };

            if let Some(level) = heading_level {
                lines.push(String::new());
                lines.push(format!("{} {}", "#".repeat(level), text));
                lines.push(String::new());
                continue;
            }
        }

        // List items
        if para.is_list_item {
            let indent = "  ".repeat(para.list_level as usize);
            if para.is_numbered {
                let counter = numbered_counters.entry(para.list_level).or_insert(0);
                *counter += 1;
                lines.push(format!("{}{}. {}", indent, counter, text));
            } else {
                lines.push(format!("{}- {}", indent, text));
            }
            continue;
        }

        // Reset numbered counters when not in a list
        numbered_counters.clear();

        // Regular paragraph
        lines.push(text);
    }

    lines.join("\n").trim().to_string()
}

fn format_run_markdown(run: &DocxRun) -> String {
    let mut text = run.text.clone();
    if text.is_empty() {
        return text;
    }

    if run.strikethrough {
        text = format!("~~{}~~", text);
    }
    if run.bold && run.italic {
        text = format!("***{}***", text);
    } else if run.bold {
        text = format!("**{}**", text);
    } else if run.italic {
        text = format!("*{}*", text);
    }
    // Underline has no standard markdown — skip

    text
}

// ---------------------------------------------------------------------------
// extract_pdf
// ---------------------------------------------------------------------------

pub struct ExtractPdfNode;

#[async_trait]
impl Node for ExtractPdfNode {
    fn node_type(&self) -> &str {
        "extract_pdf"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from a PDF document"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = get_path(config, &ctx, "extract_pdf")?;
        let format = validate_format(config, "extract_pdf")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("content");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());

        let bytes = std::fs::read(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;

        // Extract text
        let text = pdf_extract::extract_text_from_mem(&bytes)
            .map_err(|e| anyhow::anyhow!("Failed to extract text from '{}': {}", path, e))?;

        let content = match format {
            "markdown" => pdf_text_to_markdown(&text),
            _ => text.clone(),
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(content));

        if let Some(meta_key) = metadata_key {
            let metadata = extract_pdf_metadata(&bytes);
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        }
        Ok(output)
    }
}

fn extract_pdf_metadata(bytes: &[u8]) -> BTreeMap<String, serde_json::Value> {
    let mut meta = BTreeMap::new();

    let doc = match lopdf::Document::load_mem(bytes) {
        Ok(doc) => doc,
        Err(_) => return meta,
    };

    // Page count
    let page_count = doc.get_pages().len();
    meta.insert("pages".to_string(), serde_json::json!(page_count));

    // Info dictionary
    if let Ok(info_ref) = doc.trailer.get(b"Info")
        && let Ok(obj_ref) = info_ref.as_reference()
        && let Ok(info_obj) = doc.get_object(obj_ref)
        && let Ok(dict) = info_obj.as_dict()
    {
        let fields = [
            (b"Title".as_slice(), "title"),
            (b"Author".as_slice(), "author"),
            (b"Subject".as_slice(), "subject"),
            (b"Keywords".as_slice(), "keywords"),
            (b"Creator".as_slice(), "creator"),
            (b"Producer".as_slice(), "producer"),
            (b"CreationDate".as_slice(), "created"),
            (b"ModDate".as_slice(), "modified"),
        ];

        for (key, label) in fields {
            if let Ok(val) = dict.get(key)
                && let Ok(bytes) = val.as_str()
            {
                let s = String::from_utf8_lossy(bytes).trim().to_string();
                if !s.is_empty() {
                    meta.insert(label.to_string(), serde_json::Value::String(s));
                }
            }
        }
    }

    meta
}

fn pdf_text_to_markdown(text: &str) -> String {
    // PDF text is layout-based, not semantic. Best-effort paragraph detection.
    let mut lines: Vec<String> = Vec::new();
    let mut current_paragraph = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current_paragraph.is_empty() {
                lines.push(current_paragraph.clone());
                lines.push(String::new());
                current_paragraph.clear();
            }
        } else {
            if !current_paragraph.is_empty() {
                current_paragraph.push(' ');
            }
            current_paragraph.push_str(trimmed);
        }
    }

    if !current_paragraph.is_empty() {
        lines.push(current_paragraph);
    }

    lines.join("\n").trim().to_string()
}

// ---------------------------------------------------------------------------
// extract_html
// ---------------------------------------------------------------------------

pub struct ExtractHtmlNode;

#[async_trait]
impl Node for ExtractHtmlNode {
    fn node_type(&self) -> &str {
        "extract_html"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from an HTML file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = get_path(config, &ctx, "extract_html")?;
        let format = validate_format(config, "extract_html")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("content");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());

        let html = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;

        let content = match format {
            "markdown" => html2md::parse_html(&html),
            _ => {
                // Strip HTML tags for plain text — sanitize with ammonia then strip
                let clean = ammonia::clean(&html);
                // ammonia keeps safe HTML; parse again with html2md for text extraction
                html2md::parse_html(&clean)
                    .lines()
                    .map(|l| l.trim())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(content));

        if let Some(meta_key) = metadata_key {
            let metadata = extract_html_metadata(&html);
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        }
        Ok(output)
    }
}

fn extract_html_metadata(html: &str) -> BTreeMap<String, String> {
    let mut meta = BTreeMap::new();

    // Extract <title> content
    if let Some(start) = html.find("<title>").or_else(|| html.find("<title ")) {
        let after_tag = &html[start..];
        if let Some(close) = after_tag.find('>') {
            let after_open = &after_tag[close + 1..];
            if let Some(end) = after_open.find("</title>") {
                let title = after_open[..end].trim().to_string();
                if !title.is_empty() {
                    meta.insert("title".to_string(), title);
                }
            }
        }
    }

    // Extract <meta> tags
    let lower = html.to_lowercase();
    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find("<meta ") {
        let abs_pos = search_from + pos;
        let tag_end = match lower[abs_pos..].find('>') {
            Some(p) => abs_pos + p + 1,
            None => break,
        };
        let tag = &html[abs_pos..tag_end];

        if let (Some(name), Some(content)) = (
            extract_attr(tag, "name").or_else(|| extract_attr(tag, "property")),
            extract_attr(tag, "content"),
        ) {
            let key = name.to_lowercase();
            match key.as_str() {
                "description" | "author" | "keywords" | "viewport" | "og:title"
                | "og:description" | "og:type" | "og:url" => {
                    meta.insert(key, content);
                }
                _ => {}
            }
        }

        search_from = tag_end;
    }

    meta
}

fn extract_attr(tag: &str, attr_name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let pattern = format!("{}=\"", attr_name);
    if let Some(start) = lower.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(end) = tag[value_start..].find('"') {
            return Some(tag[value_start..value_start + end].to_string());
        }
    }
    // Try single quotes
    let pattern = format!("{}='", attr_name);
    if let Some(start) = lower.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(end) = tag[value_start..].find('\'') {
            return Some(tag[value_start..value_start + end].to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// extract_vtt
// ---------------------------------------------------------------------------

pub struct ExtractVttNode;

#[async_trait]
impl Node for ExtractVttNode {
    fn node_type(&self) -> &str {
        "extract_vtt"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from WebVTT subtitle files"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = get_path(config, &ctx, "extract_vtt")?;
        let format = validate_format(config, "extract_vtt")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("transcript");
        let cues_key = config
            .get("cues_key")
            .and_then(|v| v.as_str())
            .unwrap_or("cues");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());

        let input = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;

        let cues = parse_subtitle_cues(&input, true);
        let content = format_caption_output(&cues, format);
        let transcript = format_caption_output(&cues, "text");
        let metadata = collect_subtitle_metadata(&cues, "vtt");
        let cues_payload = subtitle_cues_as_json(&cues);

        let mut output = NodeOutput::new();
        output.insert(
            "transcript".to_string(),
            serde_json::Value::String(transcript),
        );
        output.insert(cues_key.to_string(), serde_json::to_value(cues_payload)?);
        output.insert(output_key.to_string(), serde_json::Value::String(content));
        if let Some(meta_key) = metadata_key {
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        }
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// extract_srt
// ---------------------------------------------------------------------------

pub struct ExtractSrtNode;

#[async_trait]
impl Node for ExtractSrtNode {
    fn node_type(&self) -> &str {
        "extract_srt"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from SRT subtitle files"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = get_path(config, &ctx, "extract_srt")?;
        let format = validate_format(config, "extract_srt")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("transcript");
        let cues_key = config
            .get("cues_key")
            .and_then(|v| v.as_str())
            .unwrap_or("cues");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());

        let input = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;

        let cues = parse_subtitle_cues(&input, false);
        let content = format_caption_output(&cues, format);
        let transcript = format_caption_output(&cues, "text");
        let metadata = collect_subtitle_metadata(&cues, "srt");
        let cues_payload = subtitle_cues_as_json(&cues);

        let mut output = NodeOutput::new();
        output.insert(
            "transcript".to_string(),
            serde_json::Value::String(transcript),
        );
        output.insert(cues_key.to_string(), serde_json::to_value(cues_payload)?);
        output.insert(output_key.to_string(), serde_json::Value::String(content));
        if let Some(meta_key) = metadata_key {
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        }
        Ok(output)
    }
}

#[derive(Clone)]
struct SubtitleCue {
    start_ms: u64,
    end_ms: u64,
    text: String,
}

fn parse_subtitle_cues(contents: &str, is_vtt: bool) -> Vec<SubtitleCue> {
    let mut cues = Vec::new();
    let mut lines = contents.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if is_vtt {
            if trimmed == "WEBVTT" {
                continue;
            }
            if trimmed.starts_with("NOTE") {
                for next in lines.by_ref() {
                    if next.trim().is_empty() {
                        break;
                    }
                }
                continue;
            }
        }

        let Some((start_ms, end_ms)) = parse_caption_range(trimmed) else {
            continue;
        };

        let mut text_lines = Vec::new();
        for candidate in lines.by_ref() {
            if candidate.trim().is_empty() {
                break;
            }
            text_lines.push(candidate.to_string());
        }
        if text_lines.is_empty() {
            continue;
        }

        let raw_text = text_lines
            .into_iter()
            .map(|line| remove_annotation_tags(&line))
            .collect::<Vec<_>>()
            .join(" ");
        let text = raw_text.replace('\u{feff}', "").trim().to_string();
        if !text.is_empty() {
            cues.push(SubtitleCue {
                start_ms,
                end_ms,
                text,
            });
        }
    }

    cues
}

fn parse_caption_range(line: &str) -> Option<(u64, u64)> {
    if line.is_empty() {
        return None;
    }

    if !line.contains("-->") {
        return None;
    }

    let mut parts = line.splitn(2, "-->");
    let start_text = parts.next()?.trim();
    let rest = parts.next()?.trim();
    if rest.is_empty() {
        return None;
    }

    let mut time_parts = rest.split_whitespace();
    let end_text = time_parts.next()?;
    let start_ms = parse_timestamp_ms(start_text)?;
    let end_ms = parse_timestamp_ms(end_text)?;
    if end_ms < start_ms {
        None
    } else {
        Some((start_ms, end_ms))
    }
}

fn parse_timestamp_ms(value: &str) -> Option<u64> {
    let normalized = value.replace(',', ".");
    let mut timestamp_and_ms = normalized.split('.');
    let hms_part = timestamp_and_ms.next()?;
    let ms_part = timestamp_and_ms.next().unwrap_or("000");

    let hms_parts: Vec<u64> = hms_part
        .split(':')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;

    let (hours, minutes, seconds) = match hms_parts.as_slice() {
        [h, m, s] => (*h, *m, *s),
        [m, s] => (0, *m, *s),
        _ => return None,
    };

    if minutes > 59 || seconds > 59 {
        return None;
    }

    let mut millis = ms_part.chars().take(3).collect::<String>();
    while millis.len() < 3 {
        millis.push('0');
    }
    if millis.len() > 3 {
        millis.truncate(3);
    }

    let millis = millis.parse::<u64>().ok()?;
    Some(((hours * 3600 + minutes * 60 + seconds) * 1000) + millis)
}

fn format_timestamp(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let milliseconds = ms % 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{milliseconds:03}")
}

fn subtitle_cues_as_json(cues: &[SubtitleCue]) -> Vec<serde_json::Value> {
    cues.iter()
        .map(|cue| {
            serde_json::json!({
                "start_ms": cue.start_ms,
                "end_ms": cue.end_ms,
                "start": format_timestamp(cue.start_ms),
                "end": format_timestamp(cue.end_ms),
                "text": cue.text,
            })
        })
        .collect()
}

fn format_caption_output(cues: &[SubtitleCue], format: &str) -> String {
    if cues.is_empty() {
        return String::new();
    }

    match format {
        "markdown" => cues
            .iter()
            .map(|cue| {
                format!(
                    "- `{}` -> `{}`: {}",
                    format_timestamp(cue.start_ms),
                    format_timestamp(cue.end_ms),
                    cue.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => cues
            .iter()
            .map(|cue| cue.text.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn collect_subtitle_metadata(
    cues: &[SubtitleCue],
    format_name: &str,
) -> BTreeMap<String, serde_json::Value> {
    let mut metadata = BTreeMap::new();
    metadata.insert("type".to_string(), serde_json::json!(format_name));
    let cue_count = u64::try_from(cues.len()).unwrap_or(u64::MAX);
    metadata.insert("cue_count".to_string(), serde_json::json!(cue_count));

    let first_start_ms = cues.first().map_or(0, |cue| cue.start_ms);
    if first_start_ms > 0 {
        metadata.insert(
            "first_start_ms".to_string(),
            serde_json::json!(first_start_ms),
        );
    }
    if let Some(last) = cues.last() {
        metadata.insert("last_end_ms".to_string(), serde_json::json!(last.end_ms));
        metadata.insert(
            "duration_ms".to_string(),
            serde_json::json!(last.end_ms.saturating_sub(first_start_ms)),
        );
    }
    metadata
}

fn remove_annotation_tags(value: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag {
            out.push(ch);
        }
    }
    out
}
