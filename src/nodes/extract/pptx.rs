use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;

use crate::artifacts::LocalArtifactStore;
use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};
use crate::util::node_config::{config_bool_or, get_path};

use super::common::{ensure_distinct_keys, optional_string, string_or, validate_word_format};
use super::ooxml::Archive;
use super::pptx_format::{
    pptx_comments_to_json, pptx_slides_into_json, pptx_slides_to_markdown, pptx_slides_to_text,
};
use super::pptx_parser::{
    PptxComment, extract_pptx_comments, extract_pptx_metadata, extract_pptx_slides,
};
use super::resource::{Budget, Limits};

pub struct ExtractPptxNode;

struct Request {
    path: PathBuf,
    format: String,
    output_key: String,
    metadata_key: Option<String>,
    comments_key: Option<String>,
    media_mode: MediaMode,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum MediaMode {
    None,
    Artifact,
}

#[async_trait]
impl Node for ExtractPptxNode {
    fn node_type(&self) -> &str {
        "extract_pptx"
    }

    fn description(&self) -> &str {
        "Extract slides, speaker notes, and comments from a PowerPoint (.pptx) deck"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let format = validate_word_format(config, "extract_pptx")?.to_string();
        if config_bool_or(config, "include_image_bytes", ctx, false)? {
            anyhow::bail!(
                "extract_pptx: 'include_image_bytes = true' is no longer supported; use \
                 'media_mode = \"artifact\"' with format = 'json'"
            );
        }
        let media_mode = match string_or(config, "media_mode", "none", "extract_pptx")? {
            "none" => MediaMode::None,
            "artifact" => MediaMode::Artifact,
            other => anyhow::bail!(
                "extract_pptx: unsupported media_mode '{other}'. Must be 'none' or 'artifact'."
            ),
        };
        if media_mode == MediaMode::Artifact && format != "json" {
            anyhow::bail!("extract_pptx: 'media_mode = \"artifact\"' requires format = 'json'");
        }

        let output_key = string_or(config, "output_key", "content", "extract_pptx")?.to_string();
        let metadata_key =
            optional_string(config, "metadata_key", "extract_pptx")?.map(str::to_string);
        let comments_key =
            optional_string(config, "comments_key", "extract_pptx")?.map(str::to_string);
        let mut output_keys = vec![("output_key", output_key.as_str())];
        if let Some(key) = metadata_key.as_deref() {
            output_keys.push(("metadata_key", key));
        }
        if let Some(key) = comments_key.as_deref() {
            output_keys.push(("comments_key", key));
        }
        ensure_distinct_keys("extract_pptx", &output_keys)?;

        let request = Request {
            path: PathBuf::from(get_path(config, ctx, "extract_pptx")?),
            format,
            output_key,
            metadata_key,
            comments_key,
            media_mode,
        };
        let limits = Limits::current();

        run_tracked_blocking_step(move |execution| extract(request, limits, execution)).await
    }
}

fn extract(request: Request, limits: Limits, execution: ExecutionControl) -> Result<NodeOutput> {
    let mut budget = Budget::new("extract_pptx", limits, &execution);
    let mut archive = Archive::open(&request.path, "extract_pptx", limits, &execution)?;
    let artifact_store = (request.media_mode == MediaMode::Artifact)
        .then(LocalArtifactStore::from_env)
        .transpose()?;
    let mut slides = extract_pptx_slides(
        &mut archive,
        artifact_store.as_ref(),
        &mut budget,
        &execution,
    )?;
    let comments = extract_pptx_comments(&mut archive, &mut budget, &execution)?;

    // Preserve the flat comments contract without cloning the complete slide
    // graph. Comments themselves move into their matching slide afterwards.
    let flat_comments = request
        .comments_key
        .as_ref()
        .map(|_| pptx_comments_to_json(&comments, &mut budget, true))
        .transpose()?;
    attach_comments(&mut slides, comments, &budget)?;

    let metadata = request
        .metadata_key
        .as_ref()
        .map(|_| extract_pptx_metadata(&mut archive, slides.len(), &mut budget, &execution))
        .transpose()?;

    let content = match request.format.as_str() {
        "text" => serde_json::Value::String(pptx_slides_to_text(&slides, &mut budget)?),
        "markdown" => serde_json::Value::String(pptx_slides_to_markdown(&slides, &mut budget)?),
        "json" => pptx_slides_into_json(slides, &mut budget)?,
        _ => unreachable!("format was validated before starting the worker"),
    };

    let mut output = NodeOutput::new();
    output.insert(request.output_key, content);
    if let (Some(key), Some(metadata)) = (request.metadata_key, metadata) {
        output.insert(key, serde_json::to_value(metadata)?);
    }
    if let (Some(key), Some(comments)) = (request.comments_key, flat_comments) {
        output.insert(key, comments);
    }
    budget.ensure_output(&output)?;
    Ok(output)
}

fn attach_comments(
    slides: &mut [super::pptx_parser::PptxSlide],
    comments: Vec<PptxComment>,
    budget: &Budget<'_>,
) -> Result<()> {
    let mut by_slide: HashMap<u32, Vec<PptxComment>> = HashMap::new();
    for comment in comments {
        budget.checkpoint()?;
        by_slide
            .entry(comment.slide_index)
            .or_default()
            .push(comment);
    }
    for slide in slides {
        budget.checkpoint()?;
        if let Some(comments) = by_slide.remove(&slide.slide_index) {
            slide.comments = comments;
        }
    }
    Ok(())
}
