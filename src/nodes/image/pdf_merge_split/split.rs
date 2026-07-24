use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use lopdf::{Document, Object, dictionary};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::super::common::{parse_pages_spec, resolve_path};
use super::{collect_objects_recursive, remap_references};

pub(crate) struct PdfSplitNode;

#[async_trait]
impl Node for PdfSplitNode {
    fn node_type(&self) -> &str {
        "pdf_split"
    }

    fn description(&self) -> &str {
        "Split a PDF into individual pages or page ranges"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = resolve_path(config, ctx, "pdf_split")?;
        let output_dir = config
            .get("output_dir")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("pdf_split requires 'output_dir' parameter"))?;
        let output_dir = interpolate_ctx(output_dir, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("pdf_split");
        let source = Document::load(&path).map_err(|error| {
            anyhow::anyhow!("pdf_split: failed to load '{}': {:?}", path, error)
        })?;
        let source_pages = source.get_pages();
        let pages_spec = config
            .get("pages")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let page_indices = parse_pages_spec(pages_spec, source_pages.len())?;

        std::fs::create_dir_all(&output_dir).map_err(|error| {
            anyhow::anyhow!("pdf_split: failed to create output dir: {}", error)
        })?;
        let stem = std::path::Path::new(&path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("page");
        let mut page_numbers: Vec<_> = source_pages.keys().copied().collect();
        page_numbers.sort();

        let mut output_files = Vec::new();
        for &page_index in &page_indices {
            let page_number = page_numbers.get(page_index).ok_or_else(|| {
                anyhow::anyhow!("pdf_split: page index {} out of range", page_index)
            })?;
            let page_id = source_pages[page_number];
            let mut document = single_page_document(&source, page_id);
            let output_path = format!("{}/{}_{}.pdf", output_dir, stem, page_index + 1);
            document.save(&output_path).map_err(|error| {
                anyhow::anyhow!(
                    "pdf_split: failed to save page {}: {:?}",
                    page_index + 1,
                    error
                )
            })?;
            output_files.push(serde_json::Value::String(output_path));
        }

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_files", output_key),
            serde_json::Value::Array(output_files),
        );
        output.insert(
            format!("{}_page_count", output_key),
            serde_json::json!(page_indices.len()),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

fn single_page_document(source: &Document, page_id: lopdf::ObjectId) -> Document {
    let mut document = Document::new();
    let pages_id = document.new_object_id();
    let mut objects = BTreeMap::new();
    collect_objects_recursive(source, page_id, &mut objects);
    let remap: BTreeMap<_, _> = objects
        .iter()
        .map(|(&old_id, object)| (old_id, document.add_object(object.clone())))
        .collect();
    for new_id in remap.values() {
        if let Ok(object) = document.get_object_mut(*new_id) {
            remap_references(object, &remap);
        }
    }

    let new_page_id = remap[&page_id];
    if let Ok(Object::Dictionary(dictionary)) = document.get_object_mut(new_page_id) {
        dictionary.set("Parent", pages_id);
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(new_page_id)],
            "Count" => 1_u32,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.max_id = document.objects.keys().map(|id| id.0).max().unwrap_or(0);
    document
}
