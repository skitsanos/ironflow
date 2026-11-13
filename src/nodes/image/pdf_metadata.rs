use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;
use crate::util::pdf_info::{INFO_FIELDS, decode_info_string};

pub(crate) struct PdfMetadataNode;

fn extract_pdf_metadata_for_node(doc: &lopdf::Document) -> BTreeMap<String, serde_json::Value> {
    let mut metadata = BTreeMap::new();
    let page_count = doc.get_pages().len();
    metadata.insert("pages".to_string(), serde_json::json!(page_count));

    // The Info entry and each of its values may be direct or indirect.
    if let Ok(info) = doc.trailer.get(b"Info")
        && let Ok((_, info)) = doc.dereference(info)
        && let Ok(dict) = info.as_dict()
    {
        for (pdf_key, label) in INFO_FIELDS {
            if let Ok(value) = dict.get(pdf_key)
                && let Ok((_, value)) = doc.dereference(value)
                && let Some(text) = decode_info_string(value)
            {
                metadata.insert(label.to_string(), serde_json::Value::String(text));
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
