use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use async_trait::async_trait;

use crate::artifacts::FileSource;
use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;

use super::common::{ensure_distinct_keys, optional_string, string_or, validate_format};
use super::resource::{Budget, Limits, read_file};
use crate::util::file_source::get_file_source;

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
        let source = get_file_source(config, ctx, "extract_pdf")?;
        let markdown = validate_format(config, "extract_pdf")? == "markdown";
        let output_key = string_or(config, "output_key", "content", "extract_pdf")?.to_string();
        let metadata_key =
            optional_string(config, "metadata_key", "extract_pdf")?.map(str::to_string);
        if let Some(metadata_key) = metadata_key.as_deref() {
            ensure_distinct_keys(
                "extract_pdf",
                &[("output_key", &output_key), ("metadata_key", metadata_key)],
            )?;
        }
        let limits = Limits::current();

        run_tracked_blocking_step(move |execution| {
            extract(
                &source,
                markdown,
                output_key,
                metadata_key,
                limits,
                execution,
            )
        })
        .await
    }
}

fn extract(
    source: &FileSource,
    markdown: bool,
    output_key: String,
    metadata_key: Option<String>,
    limits: Limits,
    execution: crate::util::execution::ExecutionControl,
) -> Result<NodeOutput> {
    let mut budget = Budget::new("extract_pdf", limits, &execution);
    budget.checkpoint()?;
    let bytes = read_file(
        source,
        crate::util::limits::max_pdf_bytes(),
        "extract_pdf",
        &execution,
    )?;

    // lopdf and pdf_extract are synchronous, opaque parsers. They cannot be
    // interrupted inside a library call, so each call is bounded by input and
    // output limits and bracketed by cooperative cancellation checkpoints.
    budget.checkpoint()?;
    let document = lopdf::Document::load_mem(&bytes).with_context(|| {
        format!(
            "extract_pdf: failed to parse PDF '{}' for page limits",
            "verified input"
        )
    })?;
    budget.checkpoint()?;

    let page_count = document.get_pages().len() as u64;
    if page_count > limits.max_pdf_pages {
        anyhow::bail!(
            "extract_pdf: PDF has {} pages, exceeds IRONFLOW_MAX_PDF_EXTRACT_PAGES ({})",
            page_count,
            limits.max_pdf_pages
        );
    }
    budget.charge_items(page_count, "PDF pages")?;

    let metadata = metadata_key
        .as_ref()
        .map(|_| extract_pdf_metadata(&document, page_count, &mut budget))
        .transpose()?;
    // pdf_extract reparses the byte buffer internally. Release lopdf's object
    // graph first so the two parsed representations are not resident together.
    drop(document);

    budget.checkpoint()?;
    let text = pdf_extract::extract_text_from_mem(&bytes).with_context(|| {
        format!(
            "extract_pdf: failed to extract text from '{}'",
            "verified input"
        )
    })?;
    budget.checkpoint()?;
    inspect_text(&text, &mut budget)?;

    let content = if markdown {
        pdf_text_to_markdown(&text, &mut budget)?
    } else {
        budget.charge_output(text.len() as u64, "PDF extracted content")?;
        text
    };

    let mut output = NodeOutput::new();
    output.insert(output_key, serde_json::Value::String(content));
    if let (Some(key), Some(metadata)) = (metadata_key, metadata) {
        output.insert(key, serde_json::to_value(metadata)?);
    }
    budget.ensure_output(&output)?;
    Ok(output)
}

fn extract_pdf_metadata(
    document: &lopdf::Document,
    page_count: u64,
    budget: &mut Budget<'_>,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let mut metadata = BTreeMap::new();
    metadata.insert("pages".to_string(), serde_json::json!(page_count));

    if !document.trailer.has(b"Info") {
        return Ok(metadata);
    }

    budget.checkpoint()?;
    let info = document
        .trailer
        .get(b"Info")
        .context("extract_pdf: failed to read the PDF Info entry")?;
    let (_, info) = document
        .dereference(info)
        .context("extract_pdf: failed to resolve the PDF Info entry")?;
    let dictionary = info
        .as_dict()
        .context("extract_pdf: PDF Info entry is not a dictionary")?;

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
        budget.checkpoint()?;
        if !dictionary.has(pdf_key) {
            continue;
        }
        budget.charge_item("PDF metadata fields")?;
        let value = dictionary
            .get(pdf_key)
            .with_context(|| format!("extract_pdf: failed to read PDF metadata field {label}"))?;
        let (_, value) = document.dereference(value).with_context(|| {
            format!("extract_pdf: failed to resolve PDF metadata field {label}")
        })?;
        let bytes = value
            .as_str()
            .with_context(|| format!("extract_pdf: PDF metadata field {label} is not a string"))?;
        let value = String::from_utf8_lossy(bytes).trim().to_string();
        if !value.is_empty() {
            metadata.insert(label.to_string(), serde_json::Value::String(value));
        }
    }

    Ok(metadata)
}

fn inspect_text(text: &str, budget: &mut Budget<'_>) -> Result<()> {
    for _ in text.lines() {
        budget.charge_item("PDF text lines")?;
    }
    Ok(())
}

fn pdf_text_to_markdown(text: &str, budget: &mut Budget<'_>) -> Result<String> {
    let mut output = String::new();
    let mut in_paragraph = false;

    for line in text.lines() {
        budget.checkpoint()?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            in_paragraph = false;
            continue;
        }

        let separator = if in_paragraph {
            " "
        } else if output.is_empty() {
            ""
        } else {
            "\n\n"
        };
        let rendered_bytes = separator.len().saturating_add(trimmed.len());
        budget.charge_output(rendered_bytes as u64, "PDF extracted content")?;
        output.try_reserve_exact(rendered_bytes)?;
        output.push_str(separator);
        output.push_str(trimmed);
        in_paragraph = true;
    }

    Ok(output)
}
