use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;

pub(crate) struct PdfMetadataNode;

fn extract_pdf_metadata_for_node(doc: &lopdf::Document) -> BTreeMap<String, serde_json::Value> {
    let mut metadata = BTreeMap::new();
    let page_count = doc.get_pages().len();
    metadata.insert("pages".to_string(), serde_json::json!(page_count));

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

        for (pdf_key, label) in fields {
            if let Ok(val) = dict.get(pdf_key)
                && let Ok(bytes) = val.as_str()
            {
                let s = String::from_utf8_lossy(bytes).trim().to_string();
                if !s.is_empty() {
                    metadata.insert(label.to_string(), serde_json::Value::String(s));
                }
            }
        }
    }

    metadata
}

#[async_trait]
impl Node for PdfMetadataNode {
    fn node_type(&self) -> &str {
        "pdf_metadata"
    }

    fn description(&self) -> &str {
        "Extract PDF metadata and page count"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = super::common::resolve_source(config, ctx, "pdf_metadata")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("metadata")
            .to_owned();

        run_tracked_blocking_step(move |execution| {
            let document = super::pdf_input::load_document(&source, "pdf_metadata", &execution)?;
            let metadata = extract_pdf_metadata_for_node(&document);
            execution.checkpoint()?;
            let mut output = NodeOutput::new();
            output.insert(output_key, serde_json::to_value(metadata)?);
            Ok(output)
        })
        .await
    }
}
