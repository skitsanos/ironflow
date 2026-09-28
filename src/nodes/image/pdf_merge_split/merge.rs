mod config;
mod graph;

use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use lopdf::{Document, Object, dictionary};

use crate::artifacts::FileSource;
use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::nodes::file::RootedDir;
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};

use self::config::{optional_string, parse_sources, required_string};
use self::graph::merge_source;

pub(crate) struct PdfMergeNode;

#[derive(Clone, Copy)]
struct Limits {
    files: u64,
    per_file_bytes: u64,
    total_bytes: u64,
    pages: u64,
    objects: u64,
}

impl Limits {
    fn current() -> Self {
        Self {
            files: crate::util::limits::max_pdf_merge_files(),
            per_file_bytes: crate::util::limits::max_pdf_bytes(),
            total_bytes: crate::util::limits::max_pdf_merge_bytes(),
            pages: crate::util::limits::max_pdf_merge_pages(),
            objects: crate::util::limits::max_pdf_merge_objects(),
        }
    }
}

struct Request {
    sources: Vec<FileSource>,
    output_path: PathBuf,
    limits: Limits,
}

#[async_trait]
impl Node for PdfMergeNode {
    fn node_type(&self) -> &str {
        "pdf_merge"
    }

    fn description(&self) -> &str {
        "Merge bounded PDF path or artifact sources into one PDF"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let limits = Limits::current();
        let sources = parse_sources(config, ctx, limits.files)?;
        let output_path = PathBuf::from(crate::lua::interpolate::interpolate_ctx(
            required_string(config, "output_path")?,
            ctx,
        ));
        let output_key = optional_string(config, "output_key")?
            .unwrap_or("pdf_merge")
            .to_owned();
        let result_path = output_path.to_string_lossy().into_owned();
        let page_count = run_tracked_blocking_step(move |execution| {
            merge(
                Request {
                    sources,
                    output_path,
                    limits,
                },
                &execution,
            )
            .map(|count| (count, output_key))
        })
        .await?;

        Ok(NodeOutput::from([
            (
                format!("{}_path", page_count.1),
                serde_json::Value::String(result_path),
            ),
            (
                format!("{}_page_count", page_count.1),
                serde_json::json!(page_count.0),
            ),
            (
                format!("{}_success", page_count.1),
                serde_json::Value::Bool(true),
            ),
        ]))
    }
}

fn merge(request: Request, execution: &ExecutionControl) -> Result<u64> {
    let mut merged = Document::new();
    let pages_id = merged.new_object_id();
    let mut merged_page_ids = Vec::new();
    let mut total_bytes = 0_u64;
    let mut total_pages = 0_u64;
    let mut total_objects = 2_u64;

    for source in &request.sources {
        execution.checkpoint()?;
        let loaded = crate::nodes::image::pdf_input::load_document_bounded(
            source,
            "pdf_merge",
            request.limits.per_file_bytes,
            execution,
        )?;
        total_bytes = total_bytes
            .checked_add(loaded.input_bytes)
            .ok_or_else(|| anyhow::anyhow!("pdf_merge: cumulative input byte overflow"))?;
        enforce_limit(
            total_bytes,
            request.limits.total_bytes,
            "IRONFLOW_MAX_PDF_MERGE_BYTES",
            "input bytes",
        )?;
        let page_ids = ordered_pages(&loaded.document);
        total_pages = total_pages
            .checked_add(page_ids.len() as u64)
            .ok_or_else(|| anyhow::anyhow!("pdf_merge: cumulative page count overflow"))?;
        enforce_limit(
            total_pages,
            request.limits.pages,
            "IRONFLOW_MAX_PDF_MERGE_PAGES",
            "pages",
        )?;
        merge_source(
            loaded.document,
            &page_ids,
            &mut merged,
            pages_id,
            &mut merged_page_ids,
            &mut total_objects,
            request.limits.objects,
            execution,
        )?;
    }

    finish_document(&mut merged, pages_id, merged_page_ids, total_pages)?;
    save_atomic(
        &mut merged,
        &request.output_path,
        request.limits.total_bytes,
        execution,
    )?;
    Ok(total_pages)
}

fn ordered_pages(document: &Document) -> Vec<lopdf::ObjectId> {
    let mut pages: Vec<_> = document.get_pages().into_iter().collect();
    pages.sort_by_key(|(number, _)| *number);
    pages.into_iter().map(|(_, id)| id).collect()
}

fn finish_document(
    merged: &mut Document,
    pages_id: lopdf::ObjectId,
    page_ids: Vec<lopdf::ObjectId>,
    page_count: u64,
) -> Result<()> {
    let count = u32::try_from(page_count)
        .map_err(|_| anyhow::anyhow!("pdf_merge: page count exceeds PDF u32 range"))?;
    merged.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => count,
        }),
    );
    let catalog_id = merged.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.keys().map(|id| id.0).max().unwrap_or(0);
    Ok(())
}

fn save_atomic(
    document: &mut Document,
    destination: &Path,
    maximum: u64,
    execution: &ExecutionControl,
) -> Result<()> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let leaf = destination
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("pdf_merge: output path has no file name"))?;
    let root = RootedDir::prepare(parent, "pdf_merge", execution)?;
    super::output::save_atomic(
        document,
        &root,
        Path::new(leaf),
        super::output::Policy {
            operation: "pdf_merge",
            variable: "IRONFLOW_MAX_PDF_MERGE_BYTES",
            maximum,
            overwrite: true,
        },
        execution,
    )
}

fn enforce_limit(value: u64, maximum: u64, variable: &str, label: &str) -> Result<()> {
    if value > maximum {
        anyhow::bail!("pdf_merge: {label} {value} exceed {variable} ({maximum})");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
