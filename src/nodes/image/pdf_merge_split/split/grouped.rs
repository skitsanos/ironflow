use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use lopdf::{Document, ObjectId};
use serde_json::{Value, json};

use crate::engine::types::NodeOutput;
use crate::nodes::file::RootedDir;
use crate::util::{execution::ExecutionControl, limits};

use super::super::output::{Policy, save_atomic};
use super::{Request, document::selected_document};

pub(super) fn parse_size(config: &Value) -> Result<usize> {
    let Some(value) = config.get("pages_per_file") else {
        return Ok(1);
    };
    let maximum = limits::max_pdf_split_pages();
    let size = value.as_u64().filter(|size| *size > 0 && *size <= maximum)
        .ok_or_else(|| anyhow::anyhow!(
            "pdf_split: 'pages_per_file' must be an integer from 1 to IRONFLOW_MAX_PDF_SPLIT_PAGES ({maximum})"
        ))?;
    usize::try_from(size)
        .map_err(|_| anyhow::anyhow!("pdf_split: 'pages_per_file' exceeds platform range"))
}

pub(super) fn split(
    source: &Document,
    request: &Request,
    page_ids: &[ObjectId],
    indices: &[usize],
    execution: &ExecutionControl,
) -> Result<NodeOutput> {
    let mut seen = BTreeSet::new();
    for &index in indices {
        execution.checkpoint()?;
        if !seen.insert(page_ids[index]) {
            anyhow::bail!(
                "pdf_split: duplicate page {} is not supported with pages_per_file > 1",
                index + 1
            );
        }
    }
    let root = RootedDir::prepare(Path::new(&request.output_dir), "pdf_split", execution)?;
    let count = indices.len().div_ceil(request.pages_per_file);
    let mut files = Vec::new();
    let mut parts = Vec::new();
    files.try_reserve_exact(count)?;
    parts.try_reserve_exact(count)?;
    let maximum_objects = limits::max_pdf_objects();
    let maximum_bytes = limits::max_pdf_bytes();
    for (index, group) in indices.chunks(request.pages_per_file).enumerate() {
        execution.checkpoint()?;
        let selected: Vec<_> = group.iter().map(|&index| page_ids[index]).collect();
        let mut document = selected_document(source, &selected, Some(maximum_objects), execution)?;
        let leaf = format!("{}_part_{:03}.pdf", request.stem, index + 1);
        save_atomic(
            &mut document,
            &root,
            Path::new(&leaf),
            Policy {
                operation: "pdf_split",
                variable: "IRONFLOW_MAX_PDF_BYTES",
                maximum: maximum_bytes,
                overwrite: false,
            },
            execution,
        )?;
        let path = Path::new(&request.output_dir)
            .join(leaf)
            .to_string_lossy()
            .into_owned();
        let pages: Vec<_> = group.iter().map(|index| index + 1).collect();
        parts.push(json!({"path": path, "pages": pages, "page_count": group.len()}));
        files.push(Value::String(path));
    }
    execution.checkpoint()?;
    let mut output = super::result(&request.output_key, files, indices.len());
    output.insert(format!("{}_parts", request.output_key), Value::Array(parts));
    Ok(output)
}
