use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::common::{get_path, validate_format};

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
