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

/// Validate the `format` parameter for extract_word — also accepts "json".
fn validate_word_format<'a>(config: &'a serde_json::Value, node_name: &str) -> Result<&'a str> {
    let format = config
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    match format {
        "text" | "markdown" | "json" => Ok(format),
        other => anyhow::bail!(
            "{}: unsupported format '{}'. Must be 'text', 'markdown', or 'json'.",
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

/// Structured representation of a DOCX paragraph.
#[derive(Default, Clone)]
struct DocxParagraph {
    style: Option<String>,
    runs: Vec<DocxRun>,
    is_list_item: bool,
    list_level: u32,
    is_numbered: bool,
}

#[derive(Default, Clone)]
struct DocxRun {
    text: String,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    /// Resolved hex color (uppercase, no leading '#'), e.g. "0066FF". None if not set, "auto", or unresolved.
    color: Option<String>,
    /// Highlight (background) color name as defined in OOXML, e.g. "yellow".
    highlight: Option<String>,
}

#[derive(Default, Clone)]
struct DocxTable {
    rows: Vec<DocxRow>,
}

#[derive(Default, Clone)]
struct DocxRow {
    cells: Vec<DocxCell>,
}

#[derive(Default, Clone)]
struct DocxCell {
    paragraphs: Vec<DocxParagraph>,
}

#[derive(Clone)]
enum DocxBlock {
    Paragraph(DocxParagraph),
    Table(DocxTable),
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

/// Parse word/theme/theme1.xml and return a map from themeColor name → hex string.
/// Theme color names align with OOXML w:themeColor values: "dark1", "light1", "dark2", "light2",
/// "accent1"..."accent6", "hyperlink", "followedHyperlink". Returns an empty map if the theme file
/// is absent or malformed.
fn parse_theme_colors(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let xml = match archive.by_name("word/theme/theme1.xml") {
        Ok(mut entry) => {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                buf
            } else {
                return map;
            }
        }
        Err(_) => return map,
    };

    // Maps clrScheme child element names to themeColor canonical names used in document.xml.
    // OOXML uses dk1/lt1/dk2/lt2 in theme1.xml; w:themeColor uses dark1/light1/dark2/light2.
    let translate = |local: &str| -> Option<&'static str> {
        match local {
            "dk1" => Some("dark1"),
            "lt1" => Some("light1"),
            "dk2" => Some("dark2"),
            "lt2" => Some("light2"),
            "accent1" => Some("accent1"),
            "accent2" => Some("accent2"),
            "accent3" => Some("accent3"),
            "accent4" => Some("accent4"),
            "accent5" => Some("accent5"),
            "accent6" => Some("accent6"),
            "hlink" => Some("hyperlink"),
            "folHlink" => Some("followedHyperlink"),
            _ => None,
        }
    };

    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();

    let mut in_clr_scheme = false;
    let mut current_role: Option<&'static str> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);

                if local == "clrScheme" {
                    in_clr_scheme = true;
                } else if in_clr_scheme {
                    if let Some(role) = translate(local) {
                        current_role = Some(role);
                    } else if (local == "srgbClr" || local == "sysClr")
                        && let Some(role) = current_role
                    {
                        let attr_name: &[u8] = if local == "srgbClr" {
                            b"val"
                        } else {
                            b"lastClr"
                        };
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == attr_name {
                                let v = String::from_utf8_lossy(&attr.value).to_uppercase();
                                map.insert(role.to_string(), v);
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "clrScheme" {
                    in_clr_scheme = false;
                    current_role = None;
                } else if translate(local).is_some() {
                    current_role = None;
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

/// Walk word/document.xml and emit blocks (paragraphs + tables) in document order.
/// Captures run-level bold/italic/underline/strike, color (with theme resolution), highlight,
/// and paragraph style / list state. Tables nested inside cells are flattened — their paragraphs
/// become direct cell paragraphs and no nested table block is emitted.
fn parse_docx_blocks(
    xml: &str,
    numbering_defs: &std::collections::HashMap<String, bool>,
    theme_colors: &std::collections::HashMap<String, String>,
) -> Vec<DocxBlock> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut blocks: Vec<DocxBlock> = Vec::new();

    // Element-state flags
    let mut in_paragraph = false;
    let mut in_run = false;
    let mut in_run_props = false;
    let mut in_para_props = false;

    // Table state — table_depth tracks nesting depth (0 = not in any table, 1 = top-level table,
    // 2+ = nested). Open tables and rows are stacked so we can emit them on close.
    let mut table_stack: Vec<DocxTable> = Vec::new();
    let mut row_stack: Vec<DocxRow> = Vec::new();
    // Current cell paragraphs (the innermost cell currently being filled).
    let mut cell_stack: Vec<DocxCell> = Vec::new();

    let mut current_para = DocxParagraph::default();
    let mut current_run = DocxRun::default();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match name.as_str() {
                    // ---- Tables ----
                    "w:tbl" => {
                        table_stack.push(DocxTable::default());
                    }
                    "w:tr" if !table_stack.is_empty() => {
                        row_stack.push(DocxRow::default());
                    }
                    "w:tc" if !row_stack.is_empty() => {
                        cell_stack.push(DocxCell::default());
                    }

                    // ---- Paragraph open ----
                    "w:p" => {
                        in_paragraph = true;
                        current_para = DocxParagraph::default();
                    }
                    "w:pPr" if in_paragraph => {
                        in_para_props = true;
                    }
                    "w:pStyle" if in_para_props => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                current_para.style =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "w:numPr" if in_para_props => {
                        current_para.is_list_item = true;
                    }
                    "w:ilvl" if in_para_props => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val"
                                && let Ok(level) =
                                    String::from_utf8_lossy(&attr.value).parse::<u32>()
                            {
                                current_para.list_level = level;
                            }
                        }
                    }
                    "w:numId" if in_para_props => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                let num_id = String::from_utf8_lossy(&attr.value).to_string();
                                current_para.is_numbered =
                                    numbering_defs.get(&num_id).copied().unwrap_or(false);
                            }
                        }
                    }

                    // ---- Run open ----
                    "w:r" if in_paragraph => {
                        in_run = true;
                        current_run = DocxRun::default();
                    }
                    "w:rPr" if in_run => {
                        in_run_props = true;
                    }
                    "w:b" if in_run_props => {
                        current_run.bold = true;
                    }
                    "w:i" if in_run_props => {
                        current_run.italic = true;
                    }
                    "w:u" if in_run_props => {
                        current_run.underline = true;
                    }
                    "w:strike" if in_run_props => {
                        current_run.strikethrough = true;
                    }
                    "w:color" if in_run_props => {
                        let mut hex: Option<String> = None;
                        let mut theme: Option<String> = None;
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"w:val" => {
                                    let v = String::from_utf8_lossy(&attr.value).to_string();
                                    if v != "auto" && !v.is_empty() {
                                        hex = Some(v.to_uppercase());
                                    }
                                }
                                b"w:themeColor" => {
                                    theme =
                                        Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                                _ => {}
                            }
                        }
                        if let Some(h) = hex {
                            current_run.color = Some(h);
                        } else if let Some(t) = theme
                            && let Some(resolved) = theme_colors.get(&t)
                        {
                            current_run.color = Some(resolved.clone());
                        }
                    }
                    "w:highlight" if in_run_props => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                let v = String::from_utf8_lossy(&attr.value).to_string();
                                if v != "none" && !v.is_empty() {
                                    current_run.highlight = Some(v);
                                }
                            }
                        }
                    }
                    "w:tab" if in_run => {
                        current_run.text.push('\t');
                    }
                    "w:br" if in_run => {
                        current_run.text.push('\n');
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) if in_run => {
                let text = String::from_utf8_lossy(e.as_ref());
                current_run.text.push_str(&text);
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "w:p" => {
                        in_paragraph = false;
                        let finished = std::mem::take(&mut current_para);
                        // Decide where this paragraph belongs:
                        // - Inside a cell → push into innermost cell.paragraphs
                        // - Otherwise → top-level block list.
                        if let Some(cell) = cell_stack.last_mut() {
                            cell.paragraphs.push(finished);
                        } else {
                            blocks.push(DocxBlock::Paragraph(finished));
                        }
                    }
                    "w:r" => {
                        in_run = false;
                        if !current_run.text.is_empty() {
                            current_para.runs.push(std::mem::take(&mut current_run));
                        } else {
                            current_run = DocxRun::default();
                        }
                    }
                    "w:rPr" => in_run_props = false,
                    "w:pPr" => in_para_props = false,
                    "w:tc" => {
                        if let Some(cell) = cell_stack.pop()
                            && let Some(row) = row_stack.last_mut()
                        {
                            row.cells.push(cell);
                        }
                    }
                    "w:tr" => {
                        if let Some(row) = row_stack.pop()
                            && let Some(table) = table_stack.last_mut()
                        {
                            table.rows.push(row);
                        }
                    }
                    "w:tbl" => {
                        if let Some(table) = table_stack.pop() {
                            if table_stack.is_empty() {
                                // Top-level table → emit as block
                                blocks.push(DocxBlock::Table(table));
                            } else {
                                // Nested table → flatten: append each cell's paragraphs into
                                // the surrounding cell's paragraph list, in row-major order.
                                if let Some(parent_cell) = cell_stack.last_mut() {
                                    for row in table.rows {
                                        for mut cell in row.cells {
                                            parent_cell.paragraphs.append(&mut cell.paragraphs);
                                        }
                                    }
                                }
                            }
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

    blocks
}

fn paragraph_plain_text(para: &DocxParagraph) -> String {
    para.runs.iter().map(|r| r.text.as_str()).collect()
}

fn blocks_to_text(blocks: &[DocxBlock]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            DocxBlock::Paragraph(p) => {
                let text = paragraph_plain_text(p);
                if !text.is_empty() || !p.runs.is_empty() {
                    lines.push(text);
                }
            }
            DocxBlock::Table(t) => {
                for row in &t.rows {
                    let row_text: Vec<String> = row
                        .cells
                        .iter()
                        .map(|c| {
                            c.paragraphs
                                .iter()
                                .map(paragraph_plain_text)
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .collect();
                    lines.push(row_text.join(" | "));
                }
            }
        }
    }
    lines.join("\n")
}

fn blocks_to_markdown(blocks: &[DocxBlock]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut numbered_counters: std::collections::HashMap<u32, u32> =
        std::collections::HashMap::new();

    for block in blocks {
        match block {
            DocxBlock::Paragraph(para) => {
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

                numbered_counters.clear();
                lines.push(text);
            }
            DocxBlock::Table(t) => {
                numbered_counters.clear();
                let rendered = table_to_markdown(t);
                if !rendered.is_empty() {
                    lines.push(String::new());
                    lines.push(rendered);
                    lines.push(String::new());
                }
            }
        }
    }

    lines.join("\n").trim().to_string()
}

fn table_to_markdown(table: &DocxTable) -> String {
    if table.rows.is_empty() {
        return String::new();
    }
    let mut out: Vec<String> = Vec::new();
    let column_count = table.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    if column_count == 0 {
        return String::new();
    }

    let render_cell = |cell: &DocxCell| -> String {
        cell.paragraphs
            .iter()
            .map(|p| p.runs.iter().map(format_run_markdown).collect::<String>())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("<br>")
            .replace('|', "\\|")
    };

    let format_row = |cells: &[DocxCell]| -> String {
        let mut parts: Vec<String> = cells.iter().map(render_cell).collect();
        while parts.len() < column_count {
            parts.push(String::new());
        }
        format!("| {} |", parts.join(" | "))
    };

    out.push(format_row(&table.rows[0].cells));
    out.push(format!(
        "|{}|",
        vec![" --- "; column_count].join("|")
    ));
    for row in &table.rows[1..] {
        out.push(format_row(&row.cells));
    }
    out.join("\n")
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

fn run_to_json(run: &DocxRun) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("text".into(), serde_json::Value::String(run.text.clone()));
    if run.bold {
        obj.insert("bold".into(), serde_json::Value::Bool(true));
    }
    if run.italic {
        obj.insert("italic".into(), serde_json::Value::Bool(true));
    }
    if run.underline {
        obj.insert("underline".into(), serde_json::Value::Bool(true));
    }
    if run.strikethrough {
        obj.insert("strike".into(), serde_json::Value::Bool(true));
    }
    if let Some(ref c) = run.color {
        obj.insert("color".into(), serde_json::Value::String(c.clone()));
    }
    if let Some(ref h) = run.highlight {
        obj.insert("highlight".into(), serde_json::Value::String(h.clone()));
    }
    serde_json::Value::Object(obj)
}

fn paragraph_to_json(para: &DocxParagraph, index: usize) -> serde_json::Value {
    let runs: Vec<serde_json::Value> = para.runs.iter().map(run_to_json).collect();
    let text = paragraph_plain_text(para);

    let mut colors: Vec<String> = para
        .runs
        .iter()
        .filter_map(|r| r.color.clone())
        .collect();
    colors.sort();
    colors.dedup();

    let mut obj = serde_json::Map::new();
    obj.insert("type".into(), serde_json::Value::String("paragraph".into()));
    obj.insert("index".into(), serde_json::json!(index));
    if let Some(ref s) = para.style {
        obj.insert("style".into(), serde_json::Value::String(s.clone()));
    }
    if para.is_list_item {
        obj.insert(
            "list".into(),
            serde_json::json!({
                "level": para.list_level,
                "numbered": para.is_numbered,
            }),
        );
    }
    if !colors.is_empty() {
        obj.insert("colors".into(), serde_json::json!(colors));
    }
    obj.insert("runs".into(), serde_json::Value::Array(runs));
    obj.insert("text".into(), serde_json::Value::String(text));
    serde_json::Value::Object(obj)
}

fn cell_to_json(cell: &DocxCell) -> serde_json::Value {
    let paragraphs: Vec<serde_json::Value> = cell
        .paragraphs
        .iter()
        .enumerate()
        .map(|(i, p)| paragraph_to_json(p, i))
        .collect();
    serde_json::json!({ "paragraphs": paragraphs })
}

fn table_to_json(table: &DocxTable, index: usize) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = table
        .rows
        .iter()
        .map(|row| {
            let cells: Vec<serde_json::Value> = row.cells.iter().map(cell_to_json).collect();
            serde_json::json!({ "cells": cells })
        })
        .collect();
    serde_json::json!({
        "type": "table",
        "index": index,
        "rows": rows,
    })
}

fn blocks_to_json(blocks: &[DocxBlock]) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| match b {
            DocxBlock::Paragraph(p) => paragraph_to_json(p, i),
            DocxBlock::Table(t) => table_to_json(t, i),
        })
        .collect();
    serde_json::json!({ "blocks": arr })
}

// ---------------------------------------------------------------------------
// extract_pptx
// ---------------------------------------------------------------------------

pub struct ExtractPptxNode;

#[async_trait]
impl Node for ExtractPptxNode {
    fn node_type(&self) -> &str {
        "extract_pptx"
    }

    fn description(&self) -> &str {
        "Extract slides, speaker notes, and comments from a PowerPoint (.pptx) deck"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_pptx")?;
        let format = validate_word_format(config, "extract_pptx")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("content");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());
        let comments_key = config.get("comments_key").and_then(|v| v.as_str());
        let include_image_bytes = config
            .get("include_image_bytes")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let file = std::fs::File::open(&path)
            .map_err(|e| anyhow::anyhow!("Failed to open '{}': {}", path, e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| anyhow::anyhow!("Failed to read PPTX archive '{}': {}", path, e))?;

        let slides = extract_pptx_slides(&mut archive, include_image_bytes);
        let comments = extract_pptx_comments(&mut archive);

        // Attach comments to their slide_index for the per-slide field, but also
        // keep a flat top-level list if comments_key is set.
        let mut slides_with_comments = slides.clone();
        for c in &comments {
            if c.slide_index >= 1 && (c.slide_index as usize) <= slides_with_comments.len() {
                slides_with_comments[(c.slide_index as usize) - 1]
                    .comments
                    .push(c.clone());
            }
        }

        let content = match format {
            "text" => serde_json::Value::String(pptx_slides_to_text(&slides_with_comments)),
            "markdown" => serde_json::Value::String(pptx_slides_to_markdown(&slides_with_comments)),
            _ => pptx_slides_to_json(&slides_with_comments),
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), content);

        if let Some(meta_key) = metadata_key {
            let metadata = extract_pptx_metadata(&mut archive, slides_with_comments.len());
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        }
        if let Some(c_key) = comments_key {
            output.insert(c_key.to_string(), serde_json::to_value(&comments)?);
        }
        Ok(output)
    }
}

#[derive(Clone)]
struct PptxSlide {
    slide_index: u32,
    title: Option<String>,
    elements: Vec<PptxElement>,
    speaker_notes: Option<String>,
    comments: Vec<PptxComment>,
}

#[derive(Clone)]
enum PptxElement {
    /// Top-level text block (could be the title placeholder or a content placeholder).
    /// `paragraphs` is a list of text paragraphs; each may be a bullet point with a level.
    TextBlock {
        placeholder: Option<String>,
        paragraphs: Vec<PptxTextPara>,
    },
    Table {
        rows: Vec<Vec<String>>,
    },
    Image {
        alt_text: Option<String>,
        embed_id: Option<String>,
        embedded_path: Option<String>,
        media_b64: Option<String>,
        mime_type: Option<String>,
    },
}

#[derive(Clone, Default)]
struct PptxTextPara {
    text: String,
    list_level: Option<u32>,
}

#[derive(Clone, serde::Serialize, Default)]
struct PptxComment {
    slide_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    idx: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initials: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    text: String,
}

fn extract_pptx_metadata(
    archive: &mut zip::ZipArchive<std::fs::File>,
    slide_count: usize,
) -> BTreeMap<String, serde_json::Value> {
    let mut meta = BTreeMap::new();
    meta.insert("slide_count".to_string(), serde_json::json!(slide_count));

    // Dublin Core metadata from docProps/core.xml (same as DOCX).
    let xml = match archive.by_name("docProps/core.xml") {
        Ok(mut entry) => {
            let mut buf = String::new();
            entry.read_to_string(&mut buf).ok();
            buf
        }
        Err(_) => return meta,
    };
    if xml.is_empty() {
        return meta;
    }
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();
    let known = [
        ("dc:title", "title"),
        ("dc:creator", "author"),
        ("dc:subject", "subject"),
        ("dc:description", "description"),
        ("cp:keywords", "keywords"),
        ("cp:lastModifiedBy", "last_modified_by"),
        ("dcterms:created", "created"),
        ("dcterms:modified", "modified"),
        ("cp:revision", "revision"),
        ("cp:category", "category"),
    ];
    let mut current_tag = String::new();
    let mut in_meta = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if known.iter().any(|(t, _)| *t == name) {
                    current_tag = name;
                    in_meta = true;
                }
            }
            Ok(Event::Text(ref e)) if in_meta => {
                let text = String::from_utf8_lossy(e.as_ref()).trim().to_string();
                if !text.is_empty()
                    && let Some((_, key)) = known.iter().find(|(t, _)| *t == current_tag)
                {
                    meta.insert(key.to_string(), serde_json::Value::String(text));
                }
            }
            Ok(Event::End(_)) => in_meta = false,
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    meta
}

fn extract_pptx_slides(
    archive: &mut zip::ZipArchive<std::fs::File>,
    include_image_bytes: bool,
) -> Vec<PptxSlide> {
    // Collect slide files by name, sorted by their numeric suffix.
    let mut slide_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    slide_names.sort_by_key(|n| {
        n.trim_start_matches("ppt/slides/slide")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(0)
    });

    let mut slides = Vec::with_capacity(slide_names.len());
    for (idx, name) in slide_names.iter().enumerate() {
        let slide_index = (idx + 1) as u32;
        let xml = {
            let mut buf = String::new();
            if let Ok(mut entry) = archive.by_name(name) {
                entry.read_to_string(&mut buf).ok();
            }
            buf
        };

        let (title, mut elements) = parse_pptx_slide(&xml);

        // Resolve image embed_id → media path via the slide's rels file.
        let rels_name = format!("ppt/slides/_rels/slide{}.xml.rels", slide_index);
        let rels = match archive.by_name(&rels_name) {
            Ok(mut entry) => {
                let mut buf = String::new();
                entry.read_to_string(&mut buf).ok();
                parse_pptx_rels(&buf)
            }
            Err(_) => std::collections::HashMap::new(),
        };
        for el in &mut elements {
            if let PptxElement::Image {
                embed_id,
                embedded_path,
                media_b64,
                mime_type,
                ..
            } = el
                && let Some(eid) = embed_id
                && let Some(target) = rels.get(eid)
            {
                // Rels targets are relative to ppt/slides/, e.g. "../media/image3.png".
                let resolved = normalize_pptx_path("ppt/slides/", target);
                *embedded_path = Some(resolved.clone());

                if include_image_bytes
                    && let Some((bytes, mt)) = read_pptx_media(archive, &resolved)
                {
                    use base64::Engine;
                    *media_b64 =
                        Some(base64::engine::general_purpose::STANDARD.encode(&bytes));
                    *mime_type = Some(mt);
                }
            }
        }

        // Try to load matching notes file.
        let notes_name = format!("ppt/notesSlides/notesSlide{}.xml", slide_index);
        let speaker_notes = match archive.by_name(&notes_name) {
            Ok(mut entry) => {
                let mut buf = String::new();
                entry.read_to_string(&mut buf).ok();
                let notes = parse_pptx_notes(&buf);
                if notes.trim().is_empty() {
                    None
                } else {
                    Some(notes)
                }
            }
            Err(_) => None,
        };

        slides.push(PptxSlide {
            slide_index,
            title,
            elements,
            speaker_notes,
            comments: Vec::new(),
        });
    }
    slides
}

/// Parse a slideN.xml.rels file → map of rId → target path.
fn parse_pptx_rels(xml: &str) -> std::collections::HashMap<String, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut map = std::collections::HashMap::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "Relationship" {
                    let mut id = String::new();
                    let mut target = String::new();
                    for attr in e.attributes().flatten() {
                        let v = String::from_utf8_lossy(&attr.value).to_string();
                        match attr.key.as_ref() {
                            b"Id" => id = v,
                            b"Target" => target = v,
                            _ => {}
                        }
                    }
                    if !id.is_empty() && !target.is_empty() {
                        map.insert(id, target);
                    }
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

/// Resolve a relative rels target ("../media/image3.png") against its source dir.
fn normalize_pptx_path(source_dir: &str, target: &str) -> String {
    // Split source_dir into components, drop empties.
    let mut parts: Vec<&str> = source_dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        if seg == ".." {
            parts.pop();
        } else if seg != "." && !seg.is_empty() {
            parts.push(seg);
        }
    }
    parts.join("/")
}

fn read_pptx_media(
    archive: &mut zip::ZipArchive<std::fs::File>,
    path: &str,
) -> Option<(Vec<u8>, String)> {
    let mut entry = archive.by_name(path).ok()?;
    let mut bytes = Vec::new();
    std::io::copy(&mut entry, &mut bytes).ok()?;
    let mime = match path.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    };
    Some((bytes, mime.to_string()))
}

/// Parse a single slide XML. Returns (title, elements).
fn parse_pptx_slide(xml: &str) -> (Option<String>, Vec<PptxElement>) {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();

    let mut title: Option<String> = None;
    let mut elements: Vec<PptxElement> = Vec::new();

    // Per-shape state
    let mut in_sp = false;
    let mut placeholder: Option<String> = None;
    let mut in_tx_body = false;
    let mut in_para = false;
    let mut current_text = String::new();
    let mut current_list_level: Option<u32> = None;
    let mut current_paras: Vec<PptxTextPara> = Vec::new();
    let mut in_run = false;
    let mut in_t = false;

    // Per-table state
    let mut in_tbl = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell_text = String::new();
    let mut in_tc = false;

    // Per-picture state
    let mut in_pic = false;
    let mut pic_alt: Option<String> = None;
    let mut pic_embed_id: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);

                // Shape boundaries
                match local {
                    "sp" => {
                        in_sp = true;
                        placeholder = None;
                        current_paras.clear();
                    }
                    "ph" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"type" {
                                placeholder =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "txBody" => {
                        in_tx_body = true;
                    }
                    "p" if in_tx_body => {
                        in_para = true;
                        current_text.clear();
                        current_list_level = None;
                    }
                    "pPr" if in_para => {}
                    "r" if in_para => {
                        in_run = true;
                    }
                    "t" if in_run => {
                        in_t = true;
                    }
                    // Table parts
                    "tbl" => {
                        in_tbl = true;
                        table_rows.clear();
                    }
                    "tr" if in_tbl => {
                        current_row.clear();
                    }
                    "tc" if in_tbl => {
                        in_tc = true;
                        current_cell_text.clear();
                    }
                    "pic" => {
                        in_pic = true;
                        pic_alt = None;
                        pic_embed_id = None;
                    }
                    "cNvPr" if in_pic => {
                        // <p:cNvPr id="..." name="..." descr="alt-text"/>
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"descr" {
                                pic_alt =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "blip" if in_pic => {
                        // <a:blip r:embed="rId3"/>
                        for attr in e.attributes().flatten() {
                            // r:embed has full key "r:embed"; check the local name
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "r:embed" || key.ends_with(":embed") {
                                pic_embed_id =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    _ => {}
                }

                // List level from pPr (a:pPr lvl="1")
                if local == "pPr" && in_para {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"lvl"
                            && let Ok(lvl) =
                                String::from_utf8_lossy(&attr.value).parse::<u32>()
                        {
                            current_list_level = Some(lvl);
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) if in_t => {
                let text = String::from_utf8_lossy(e.as_ref());
                current_text.push_str(&text);
                if in_tc {
                    current_cell_text.push_str(&text);
                }
            }
            Ok(Event::End(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                match local {
                    "t" => {
                        in_t = false;
                    }
                    "r" => {
                        in_run = false;
                    }
                    "p" if in_tx_body => {
                        if !current_text.trim().is_empty() {
                            current_paras.push(PptxTextPara {
                                text: current_text.clone(),
                                list_level: current_list_level,
                            });
                        }
                        current_text.clear();
                        in_para = false;
                    }
                    "txBody" => {
                        in_tx_body = false;
                    }
                    "sp" => {
                        // Finalize this shape's text block
                        if !current_paras.is_empty() {
                            // Title placeholder → grab as slide title
                            if placeholder.as_deref() == Some("title")
                                || placeholder.as_deref() == Some("ctrTitle")
                            {
                                if title.is_none() {
                                    title = Some(
                                        current_paras
                                            .iter()
                                            .map(|p| p.text.clone())
                                            .collect::<Vec<_>>()
                                            .join(" "),
                                    );
                                }
                            } else {
                                elements.push(PptxElement::TextBlock {
                                    placeholder: placeholder.clone(),
                                    paragraphs: current_paras.clone(),
                                });
                            }
                        }
                        in_sp = false;
                        placeholder = None;
                        current_paras.clear();
                    }
                    "tc" => {
                        in_tc = false;
                        current_row.push(current_cell_text.clone());
                        current_cell_text.clear();
                    }
                    "tr" => {
                        if !current_row.is_empty() {
                            table_rows.push(current_row.clone());
                        }
                        current_row.clear();
                    }
                    "tbl" => {
                        if !table_rows.is_empty() {
                            elements.push(PptxElement::Table {
                                rows: table_rows.clone(),
                            });
                        }
                        in_tbl = false;
                        table_rows.clear();
                    }
                    "pic" => {
                        elements.push(PptxElement::Image {
                            alt_text: pic_alt.take(),
                            embed_id: pic_embed_id.take(),
                            embedded_path: None,
                            media_b64: None,
                            mime_type: None,
                        });
                        in_pic = false;
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

    let _ = in_sp; // silence unused warning if compiler complains
    (title, elements)
}

fn parse_pptx_notes(xml: &str) -> String {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut text = String::new();
    let mut in_t = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "t" {
                    in_t = true;
                }
            }
            Ok(Event::Text(ref e)) if in_t => {
                text.push_str(&String::from_utf8_lossy(e.as_ref()));
                text.push('\n');
            }
            Ok(Event::End(_)) => {
                in_t = false;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    text.trim().to_string()
}

fn extract_pptx_comments(archive: &mut zip::ZipArchive<std::fs::File>) -> Vec<PptxComment> {
    // Author lookup (legacy: ppt/commentAuthors.xml).
    let mut authors: std::collections::HashMap<String, (Option<String>, Option<String>)> =
        std::collections::HashMap::new();
    if let Ok(mut entry) = archive.by_name("ppt/commentAuthors.xml") {
        let mut buf = String::new();
        entry.read_to_string(&mut buf).ok();
        if !buf.is_empty() {
            use quick_xml::events::Event;
            let mut reader = quick_xml::Reader::from_str(&buf);
            let mut rbuf = Vec::new();
            loop {
                match reader.read_event_into(&mut rbuf) {
                    Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                        let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        let local = raw.rsplit(':').next().unwrap_or(&raw);
                        if local == "cmAuthor" {
                            let mut id = String::new();
                            let mut name: Option<String> = None;
                            let mut initials: Option<String> = None;
                            for attr in e.attributes().flatten() {
                                let v = String::from_utf8_lossy(&attr.value).to_string();
                                match attr.key.as_ref() {
                                    b"id" => id = v,
                                    b"name" => name = Some(v),
                                    b"initials" => initials = Some(v),
                                    _ => {}
                                }
                            }
                            if !id.is_empty() {
                                authors.insert(id, (name, initials));
                            }
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                rbuf.clear();
            }
        }
    }

    // Find all ppt/comments/comment*.xml files and parse each.
    let mut comment_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.starts_with("ppt/comments/comment") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    comment_names.sort_by_key(|n| {
        n.trim_start_matches("ppt/comments/comment")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(0)
    });

    let mut out: Vec<PptxComment> = Vec::new();
    for name in &comment_names {
        // Derive slide_index from the comment file name (commentN.xml ↔ slideN.xml by convention).
        let slide_index = name
            .trim_start_matches("ppt/comments/comment")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(0);

        let xml = {
            let mut buf = String::new();
            if let Ok(mut entry) = archive.by_name(name) {
                entry.read_to_string(&mut buf).ok();
            }
            buf
        };
        if xml.is_empty() {
            continue;
        }

        // Parse <p:cm> entries; each has authorId, dt, idx attrs + a <p:text> child.
        use quick_xml::events::Event;
        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut rbuf = Vec::new();
        let mut current: Option<PptxComment> = None;
        let mut in_text = false;
        loop {
            match reader.read_event_into(&mut rbuf) {
                Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                    let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let local = raw.rsplit(':').next().unwrap_or(&raw);
                    if local == "cm" {
                        let mut c = PptxComment {
                            slide_index,
                            ..Default::default()
                        };
                        for attr in e.attributes().flatten() {
                            let v = String::from_utf8_lossy(&attr.value).to_string();
                            match attr.key.as_ref() {
                                b"authorId" => {
                                    if let Some((name, initials)) = authors.get(&v) {
                                        c.author = name.clone();
                                        c.initials = initials.clone();
                                    }
                                    c.author_id = Some(v);
                                }
                                b"dt" => c.date = Some(v),
                                b"idx" => c.idx = Some(v),
                                _ => {}
                            }
                        }
                        current = Some(c);
                    } else if local == "text" {
                        in_text = current.is_some();
                    }
                }
                Ok(Event::Text(ref e)) if in_text => {
                    if let Some(c) = current.as_mut() {
                        let t = String::from_utf8_lossy(e.as_ref());
                        c.text.push_str(&t);
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let local = raw.rsplit(':').next().unwrap_or(&raw);
                    if local == "text" {
                        in_text = false;
                    } else if local == "cm"
                        && let Some(c) = current.take()
                    {
                        out.push(c);
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            rbuf.clear();
        }
    }

    out
}

fn pptx_slides_to_text(slides: &[PptxSlide]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for s in slides {
        parts.push(format!("===== SLIDE {} =====", s.slide_index));
        if let Some(ref t) = s.title {
            parts.push(t.clone());
        }
        for el in &s.elements {
            match el {
                PptxElement::TextBlock { paragraphs, .. } => {
                    for p in paragraphs {
                        parts.push(p.text.clone());
                    }
                }
                PptxElement::Table { rows } => {
                    for row in rows {
                        parts.push(row.join(" | "));
                    }
                }
                PptxElement::Image { .. } => {}
            }
        }
        if let Some(ref n) = s.speaker_notes {
            parts.push("--- NOTES ---".to_string());
            parts.push(n.clone());
        }
        for c in &s.comments {
            parts.push(format!(
                "[COMMENT by {}{}]: {}",
                c.author.clone().unwrap_or_else(|| "?".into()),
                c.date.clone().map(|d| format!(" @ {}", d)).unwrap_or_default(),
                c.text
            ));
        }
    }
    parts.join("\n")
}

fn pptx_slides_to_markdown(slides: &[PptxSlide]) -> String {
    let mut out: Vec<String> = Vec::new();
    for s in slides {
        out.push(format!("## Slide {}", s.slide_index));
        out.push(String::new());
        if let Some(ref t) = s.title {
            out.push(format!("### {}", t));
            out.push(String::new());
        }
        for el in &s.elements {
            match el {
                PptxElement::TextBlock { paragraphs, .. } => {
                    for p in paragraphs {
                        if let Some(lvl) = p.list_level {
                            let indent = "  ".repeat(lvl as usize);
                            out.push(format!("{}- {}", indent, p.text));
                        } else {
                            out.push(p.text.clone());
                        }
                    }
                    out.push(String::new());
                }
                PptxElement::Table { rows } => {
                    if !rows.is_empty() {
                        out.push(format!("| {} |", rows[0].join(" | ")));
                        out.push(format!(
                            "|{}|",
                            vec![" --- "; rows[0].len()].join("|")
                        ));
                        for row in &rows[1..] {
                            out.push(format!("| {} |", row.join(" | ")));
                        }
                        out.push(String::new());
                    }
                }
                PptxElement::Image { .. } => {
                    out.push("*(image)*".to_string());
                }
            }
        }
        if let Some(ref n) = s.speaker_notes {
            out.push("**Speaker notes:**".to_string());
            out.push(n.clone());
            out.push(String::new());
        }
        for c in &s.comments {
            out.push(format!(
                "> 💬 **{}**{}: {}",
                c.author.clone().unwrap_or_else(|| "?".into()),
                c.date.clone().map(|d| format!(" ({})", d)).unwrap_or_default(),
                c.text
            ));
        }
    }
    out.join("\n").trim().to_string()
}

fn pptx_slides_to_json(slides: &[PptxSlide]) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = slides
        .iter()
        .map(|s| {
            let elements: Vec<serde_json::Value> = s
                .elements
                .iter()
                .map(|el| match el {
                    PptxElement::TextBlock {
                        placeholder,
                        paragraphs,
                    } => {
                        let paras: Vec<serde_json::Value> = paragraphs
                            .iter()
                            .map(|p| {
                                let mut o = serde_json::Map::new();
                                o.insert("text".into(), serde_json::Value::String(p.text.clone()));
                                if let Some(lvl) = p.list_level {
                                    o.insert("list_level".into(), serde_json::json!(lvl));
                                }
                                serde_json::Value::Object(o)
                            })
                            .collect();
                        let mut o = serde_json::Map::new();
                        o.insert("type".into(), serde_json::Value::String("text_block".into()));
                        if let Some(p) = placeholder {
                            o.insert(
                                "placeholder".into(),
                                serde_json::Value::String(p.clone()),
                            );
                        }
                        o.insert("paragraphs".into(), serde_json::Value::Array(paras));
                        serde_json::Value::Object(o)
                    }
                    PptxElement::Table { rows } => {
                        serde_json::json!({
                            "type": "table",
                            "rows": rows
                        })
                    }
                    PptxElement::Image {
                        alt_text,
                        embed_id,
                        embedded_path,
                        media_b64,
                        mime_type,
                    } => {
                        let mut o = serde_json::Map::new();
                        o.insert("type".into(), serde_json::Value::String("image".into()));
                        if let Some(a) = alt_text {
                            o.insert("alt_text".into(), serde_json::Value::String(a.clone()));
                        }
                        if let Some(e) = embed_id {
                            o.insert("embed_id".into(), serde_json::Value::String(e.clone()));
                        }
                        if let Some(p) = embedded_path {
                            o.insert(
                                "embedded_path".into(),
                                serde_json::Value::String(p.clone()),
                            );
                        }
                        if let Some(m) = media_b64 {
                            o.insert("media_b64".into(), serde_json::Value::String(m.clone()));
                        }
                        if let Some(mt) = mime_type {
                            o.insert(
                                "mime_type".into(),
                                serde_json::Value::String(mt.clone()),
                            );
                        }
                        serde_json::Value::Object(o)
                    }
                })
                .collect();
            let comments: Vec<serde_json::Value> = s
                .comments
                .iter()
                .map(|c| serde_json::to_value(c).unwrap_or(serde_json::Value::Null))
                .collect();
            let mut obj = serde_json::Map::new();
            obj.insert("slide_index".into(), serde_json::json!(s.slide_index));
            if let Some(ref t) = s.title {
                obj.insert("title".into(), serde_json::Value::String(t.clone()));
            }
            obj.insert("elements".into(), serde_json::Value::Array(elements));
            if let Some(ref n) = s.speaker_notes {
                obj.insert(
                    "speaker_notes".into(),
                    serde_json::Value::String(n.clone()),
                );
            }
            if !comments.is_empty() {
                obj.insert("comments".into(), serde_json::Value::Array(comments));
            }
            serde_json::Value::Object(obj)
        })
        .collect();
    serde_json::json!({ "slides": arr })
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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_pdf")?;
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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_html")?;
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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_vtt")?;
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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_srt")?;
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
