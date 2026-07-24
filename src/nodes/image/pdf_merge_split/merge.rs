use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use lopdf::{Document, Object, dictionary};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::{collect_objects_recursive, remap_references};

pub(crate) struct PdfMergeNode;

#[async_trait]
impl Node for PdfMergeNode {
    fn node_type(&self) -> &str {
        "pdf_merge"
    }

    fn description(&self) -> &str {
        "Merge multiple PDF files into one"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let files = config
            .get("files")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("pdf_merge requires 'files' parameter (array)"))?;
        if files.is_empty() {
            anyhow::bail!("pdf_merge: 'files' array must not be empty");
        }
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("pdf_merge requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("pdf_merge");

        let documents = load_documents(files, ctx)?;
        let (mut merged, page_count) = merge_documents(&documents);
        if let Some(parent) = std::path::Path::new(&output_path).parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                anyhow::anyhow!("pdf_merge: failed to create output directory: {}", error)
            })?;
        }
        merged.save(&output_path).map_err(|error| {
            anyhow::anyhow!("pdf_merge: failed to save merged PDF: {:?}", error)
        })?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_path", output_key),
            serde_json::Value::String(output_path),
        );
        output.insert(
            format!("{}_page_count", output_key),
            serde_json::json!(page_count),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

fn load_documents(files: &[serde_json::Value], ctx: &Context) -> Result<Vec<Document>> {
    files
        .iter()
        .map(|file| {
            let path = file.as_str().ok_or_else(|| {
                anyhow::anyhow!("pdf_merge: each entry in 'files' must be a string")
            })?;
            let path = interpolate_ctx(path, ctx);
            Document::load(&path).map_err(|error| {
                anyhow::anyhow!("pdf_merge: failed to load '{}': {:?}", path, error)
            })
        })
        .collect()
}

fn merge_documents(documents: &[Document]) -> (Document, usize) {
    let mut merged = Document::new();
    let pages_id = merged.new_object_id();
    let mut merged_page_ids = Vec::new();

    for source in documents {
        let pages = source.get_pages();
        let mut page_numbers: Vec<_> = pages.keys().copied().collect();
        page_numbers.sort();
        for page_number in page_numbers {
            let page_id = pages[&page_number];
            let mut objects = BTreeMap::new();
            collect_objects_recursive(source, page_id, &mut objects);
            let remap: BTreeMap<_, _> = objects
                .iter()
                .map(|(&old_id, object)| (old_id, merged.add_object(object.clone())))
                .collect();
            for new_id in remap.values() {
                if let Ok(object) = merged.get_object_mut(*new_id) {
                    remap_references(object, &remap);
                }
            }
            let new_page_id = remap[&page_id];
            if let Ok(Object::Dictionary(dictionary)) = merged.get_object_mut(new_page_id) {
                dictionary.set("Parent", pages_id);
            }
            merged_page_ids.push(new_page_id);
        }
    }

    let page_count = merged_page_ids.len();
    merged.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => merged_page_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => page_count as u32,
        }),
    );
    let catalog_id = merged.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.keys().map(|id| id.0).max().unwrap_or(0);
    (merged, page_count)
}
