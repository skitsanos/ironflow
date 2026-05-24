use std::collections::BTreeMap;
use std::io::Read;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::common::{get_path, validate_word_format};
use super::pptx_format::{pptx_slides_to_json, pptx_slides_to_markdown, pptx_slides_to_text};
use super::pptx_parser::{
    PptxElement, PptxSlide, extract_pptx_comments, normalize_pptx_path, parse_pptx_notes,
    parse_pptx_rels, parse_pptx_slide, read_pptx_media,
};

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
                    *media_b64 = Some(base64::engine::general_purpose::STANDARD.encode(&bytes));
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
