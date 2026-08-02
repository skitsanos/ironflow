use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use lopdf::{Document, Object, dictionary};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};

use super::super::common::{parse_pages_spec, resolve_source};
use super::{collect_objects_recursive, remap_references};

pub(crate) struct PdfSplitNode;

struct Request {
    source: crate::artifacts::FileSource,
    stem: String,
    output_dir: String,
    output_key: String,
    pages: String,
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
        let source = resolve_source(config, ctx, "pdf_split")?;
        let stem = source.file_stem("page");
        let output_dir = config
            .get("output_dir")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("pdf_split requires 'output_dir' parameter"))?;
        let output_dir = interpolate_ctx(output_dir, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("pdf_split")
            .to_owned();
        let pages = config
            .get("pages")
            .and_then(|value| value.as_str())
            .unwrap_or("all")
            .to_owned();
        run_tracked_blocking_step(move |execution| {
            split(
                Request {
                    source,
                    stem,
                    output_dir,
                    output_key,
                    pages,
                },
                &execution,
            )
        })
        .await
    }
}

fn split(request: Request, execution: &ExecutionControl) -> Result<NodeOutput> {
    let Request {
        source,
        stem,
        output_dir,
        output_key,
        pages,
    } = request;
    let source = super::super::pdf_input::load_document(&source, "pdf_split", execution)?;
    execution.checkpoint()?;
    let source_pages = source.get_pages();
    let page_indices = parse_pages_spec(
        &pages,
        source_pages.len(),
        crate::util::limits::max_pdf_split_pages(),
        "pdf_split",
        "IRONFLOW_MAX_PDF_SPLIT_PAGES",
    )?;

    std::fs::create_dir_all(&output_dir)
        .map_err(|error| anyhow::anyhow!("pdf_split: failed to create output dir: {error}"))?;
    let mut page_numbers: Vec<_> = source_pages.keys().copied().collect();
    page_numbers.sort();

    let mut output_files = Vec::new();
    output_files.try_reserve_exact(page_indices.len())?;
    for &page_index in &page_indices {
        execution.checkpoint()?;
        let page_number = page_numbers
            .get(page_index)
            .ok_or_else(|| anyhow::anyhow!("pdf_split: page index {page_index} out of range"))?;
        let page_id = source_pages[page_number];
        let mut document = single_page_document(&source, page_id);
        execution.checkpoint()?;
        let output_path =
            std::path::Path::new(&output_dir).join(format!("{stem}_{}.pdf", page_index + 1));
        document.save(&output_path).map_err(|error| {
            anyhow::anyhow!(
                "pdf_split: failed to save page {}: {error:?}",
                page_index + 1
            )
        })?;
        output_files.push(serde_json::Value::String(
            output_path.to_string_lossy().into_owned(),
        ));
    }

    execution.checkpoint()?;
    let mut output = NodeOutput::new();
    output.insert(
        format!("{output_key}_files"),
        serde_json::Value::Array(output_files),
    );
    output.insert(
        format!("{output_key}_page_count"),
        serde_json::json!(page_indices.len()),
    );
    output.insert(
        format!("{output_key}_success"),
        serde_json::Value::Bool(true),
    );
    Ok(output)
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
