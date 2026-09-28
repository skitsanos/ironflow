mod document;
mod grouped;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};

use self::document::selected_document;
use super::super::common::{parse_pages_spec, resolve_source};

pub(crate) struct PdfSplitNode;

struct Request {
    source: crate::artifacts::FileSource,
    stem: String,
    output_dir: String,
    output_key: String,
    pages: String,
    pages_per_file: usize,
}

#[async_trait]
impl Node for PdfSplitNode {
    fn node_type(&self) -> &str {
        "pdf_split"
    }

    fn description(&self) -> &str {
        "Select PDF pages and write individual pages or grouped slices"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let pages_per_file = grouped::parse_size(config)?;
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
        let pages = interpolate_ctx(&pages, ctx);
        run_tracked_blocking_step(move |execution| {
            split(
                Request {
                    source,
                    stem,
                    output_dir,
                    output_key,
                    pages,
                    pages_per_file,
                },
                &execution,
            )
        })
        .await
    }
}

fn split(request: Request, execution: &ExecutionControl) -> Result<NodeOutput> {
    let source = super::super::pdf_input::load_document(&request.source, "pdf_split", execution)?;
    execution.checkpoint()?;
    let source_pages = source.get_pages();
    let page_indices = parse_pages_spec(
        &request.pages,
        source_pages.len(),
        crate::util::limits::max_pdf_split_pages(),
        "pdf_split",
        "IRONFLOW_MAX_PDF_SPLIT_PAGES",
    )?;
    let page_ids: Vec<_> = source_pages.into_values().collect();
    if request.pages_per_file > 1 {
        return grouped::split(&source, &request, &page_ids, &page_indices, execution);
    }

    std::fs::create_dir_all(&request.output_dir)
        .map_err(|error| anyhow::anyhow!("pdf_split: failed to create output dir: {error}"))?;

    let mut output_files = Vec::new();
    output_files.try_reserve_exact(page_indices.len())?;
    for &page_index in &page_indices {
        execution.checkpoint()?;
        let page_id = page_ids
            .get(page_index)
            .ok_or_else(|| anyhow::anyhow!("pdf_split: page index {page_index} out of range"))?;
        let mut document = selected_document(&source, &[*page_id], None, execution)?;
        execution.checkpoint()?;
        let output_path = std::path::Path::new(&request.output_dir).join(format!(
            "{}_{}.pdf",
            request.stem,
            page_index + 1
        ));
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
    Ok(result(
        &request.output_key,
        output_files,
        page_indices.len(),
    ))
}

fn result(output_key: &str, output_files: Vec<serde_json::Value>, page_count: usize) -> NodeOutput {
    let mut output = NodeOutput::new();
    output.insert(
        format!("{output_key}_files"),
        serde_json::Value::Array(output_files),
    );
    output.insert(
        format!("{output_key}_page_count"),
        serde_json::json!(page_count),
    );
    output.insert(
        format!("{output_key}_success"),
        serde_json::Value::Bool(true),
    );
    output
}
