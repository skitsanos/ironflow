use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use lopdf::{Document, Object, dictionary};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::common::parse_pages_spec;

pub(crate) struct PdfMergeNode;
pub(crate) struct PdfSplitNode;

/// Recursively collect all objects referenced by a given object.
pub(crate) fn collect_objects_recursive(
    doc: &Document,
    obj_id: lopdf::ObjectId,
    collected: &mut BTreeMap<lopdf::ObjectId, Object>,
) {
    if collected.contains_key(&obj_id) {
        return;
    }
    if let Ok(obj) = doc.get_object(obj_id) {
        collected.insert(obj_id, obj.clone());
        let refs = extract_references(obj);
        for r in refs {
            collect_objects_recursive(doc, r, collected);
        }
    }
}

/// Extract all ObjectId references from an Object.
pub(crate) fn extract_references(obj: &Object) -> Vec<lopdf::ObjectId> {
    let mut refs = Vec::new();
    match obj {
        Object::Reference(id) => refs.push(*id),
        Object::Array(arr) => {
            for item in arr {
                refs.extend(extract_references(item));
            }
        }
        Object::Dictionary(dict) => {
            for (_, val) in dict.iter() {
                refs.extend(extract_references(val));
            }
        }
        Object::Stream(stream) => {
            for (_, val) in stream.dict.iter() {
                refs.extend(extract_references(val));
            }
        }
        _ => {}
    }
    refs
}

/// Remap ObjectId references within an object using the provided mapping.
pub(crate) fn remap_references(obj: &mut Object, map: &BTreeMap<lopdf::ObjectId, lopdf::ObjectId>) {
    match obj {
        Object::Reference(id) => {
            if let Some(new_id) = map.get(id) {
                *id = *new_id;
            }
        }
        Object::Array(arr) => {
            for item in arr.iter_mut() {
                remap_references(item, map);
            }
        }
        Object::Dictionary(dict) => {
            for (_, val) in dict.iter_mut() {
                remap_references(val, map);
            }
        }
        Object::Stream(stream) => {
            for (_, val) in stream.dict.iter_mut() {
                remap_references(val, map);
            }
        }
        _ => {}
    }
}

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

        let mut documents = Vec::new();
        for file_val in files {
            let path = file_val.as_str().ok_or_else(|| {
                anyhow::anyhow!("pdf_merge: each entry in 'files' must be a string")
            })?;
            let path = interpolate_ctx(path, ctx);
            let doc = Document::load(&path)
                .map_err(|e| anyhow::anyhow!("pdf_merge: failed to load '{}': {:?}", path, e))?;
            documents.push(doc);
        }

        let mut merged = Document::new();
        let mut merged_pages_ids = Vec::new();
        let pages_id = merged.new_object_id();
        let mut total_pages: usize = 0;

        for source_doc in &documents {
            let source_pages = source_doc.get_pages();
            let mut sorted_nums: Vec<u32> = source_pages.keys().copied().collect();
            sorted_nums.sort();
            for page_num in sorted_nums {
                let page_obj_id = source_pages[&page_num];
                let mut object_map = BTreeMap::new();
                collect_objects_recursive(source_doc, page_obj_id, &mut object_map);

                let mut id_remap = BTreeMap::new();
                for (&old_id, obj) in &object_map {
                    let new_id = merged.add_object(obj.clone());
                    id_remap.insert(old_id, new_id);
                }

                for new_id in id_remap.values() {
                    if let Ok(obj) = merged.get_object_mut(*new_id) {
                        remap_references(obj, &id_remap);
                    }
                }

                let new_page_id = id_remap[&page_obj_id];
                if let Ok(Object::Dictionary(dict)) = merged.get_object_mut(new_page_id) {
                    dict.set("Parent", pages_id);
                }

                merged_pages_ids.push(new_page_id);
                total_pages += 1;
            }
        }

        let kids: Vec<Object> = merged_pages_ids
            .iter()
            .map(|id| Object::Reference(*id))
            .collect();
        merged.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => total_pages as u32,
            }),
        );

        let catalog_id = merged.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        merged.trailer.set("Root", catalog_id);
        merged.max_id = merged.objects.keys().map(|id| id.0).max().unwrap_or(0);

        if let Some(parent) = std::path::Path::new(&output_path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                anyhow::anyhow!("pdf_merge: failed to create output directory: {}", e)
            })?;
        }

        merged
            .save(&output_path)
            .map_err(|e| anyhow::anyhow!("pdf_merge: failed to save merged PDF: {:?}", e))?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_path", output_key),
            serde_json::Value::String(output_path),
        );
        output.insert(
            format!("{}_page_count", output_key),
            serde_json::json!(total_pages),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[async_trait]
impl Node for PdfSplitNode {
    fn node_type(&self) -> &str {
        "pdf_split"
    }

    fn description(&self) -> &str {
        "Split a PDF into individual pages or page ranges"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = super::common::resolve_path(config, ctx, "pdf_split")?;
        let output_dir = config
            .get("output_dir")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("pdf_split requires 'output_dir' parameter"))?;
        let output_dir = interpolate_ctx(output_dir, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("pdf_split");

        let source_doc = Document::load(&path)
            .map_err(|e| anyhow::anyhow!("pdf_split: failed to load '{}': {:?}", path, e))?;

        let source_pages = source_doc.get_pages();
        let page_count = source_pages.len();

        let pages_spec = config
            .get("pages")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let page_indices = parse_pages_spec(pages_spec, page_count)?;

        std::fs::create_dir_all(&output_dir)
            .map_err(|e| anyhow::anyhow!("pdf_split: failed to create output dir: {}", e))?;

        let stem = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("page");

        let mut output_files = Vec::new();

        let mut sorted_page_nums: Vec<u32> = source_pages.keys().copied().collect();
        sorted_page_nums.sort();

        for &page_idx in &page_indices {
            let page_num = sorted_page_nums.get(page_idx).ok_or_else(|| {
                anyhow::anyhow!("pdf_split: page index {} out of range", page_idx)
            })?;
            let page_obj_id = source_pages[page_num];

            let mut single = Document::new();
            let pages_id = single.new_object_id();

            let mut object_map = BTreeMap::new();
            collect_objects_recursive(&source_doc, page_obj_id, &mut object_map);

            let mut id_remap = BTreeMap::new();
            for (&old_id, obj) in &object_map {
                let new_id = single.add_object(obj.clone());
                id_remap.insert(old_id, new_id);
            }

            for new_id in id_remap.values() {
                if let Ok(obj) = single.get_object_mut(*new_id) {
                    remap_references(obj, &id_remap);
                }
            }

            let new_page_id = id_remap[&page_obj_id];
            if let Ok(Object::Dictionary(dict)) = single.get_object_mut(new_page_id) {
                dict.set("Parent", pages_id);
            }

            single.objects.insert(
                pages_id,
                Object::Dictionary(dictionary! {
                    "Type" => "Pages",
                    "Kids" => vec![Object::Reference(new_page_id)],
                    "Count" => 1_u32,
                }),
            );

            let catalog_id = single.add_object(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
            });
            single.trailer.set("Root", catalog_id);
            single.max_id = single.objects.keys().map(|id| id.0).max().unwrap_or(0);

            let out_path = format!("{}/{}_{}.pdf", output_dir, stem, page_idx + 1);
            single.save(&out_path).map_err(|e| {
                anyhow::anyhow!("pdf_split: failed to save page {}: {:?}", page_idx + 1, e)
            })?;

            output_files.push(serde_json::Value::String(out_path));
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
