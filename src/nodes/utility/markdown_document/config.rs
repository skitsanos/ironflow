use super::language::Direction;
use super::model::{MAX_INPUT, valid_text};
use crate::{
    artifacts::{ArtifactRef, FileSource},
    engine::types::Context,
    lua::interpolate::interpolate_ctx_bounded,
    util::node_config::{config_bool_or, config_f64_or},
};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

pub(super) struct Config {
    pub input: String,
    pub path: PathBuf,
    pub output_key: String,
    pub overwrite: bool,
    pub width: f32,
    pub height: f32,
    pub margin: f32,
    pub font_size: f32,
    pub title: String,
    pub author: String,
    pub language: String,
    pub direction: Direction,
    pub images: BTreeMap<String, FileSource>,
}

impl Config {
    pub fn parse(value: &Value, ctx: &Context, operation: &str) -> Result<Self> {
        ensure!(
            value.get("input").is_some() != value.get("source_key").is_some(),
            "{operation} requires exactly one of input or source_key"
        );
        let input = if value.get("input").is_some() {
            string(value, "input", None, ctx, MAX_INPUT)?
        } else {
            let key = value["source_key"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("source_key must be a string"))?;
            let input = ctx
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("source_key must reference a Markdown string"))?;
            ensure!(
                input.len() <= MAX_INPUT,
                "Markdown exceeds {MAX_INPUT} input bytes"
            );
            input.to_owned()
        };
        let (width, height) = match string(value, "page_size", Some("a4"), ctx, 32)?
            .to_lowercase()
            .as_str()
        {
            "a4" => (595.276, 841.89),
            "letter" => (612.0, 792.0),
            _ => anyhow::bail!("page_size must be a4 or letter"),
        };
        let margin = config_f64_or(value, "margin", ctx, 54.0)?;
        ensure!(
            (18.0..=144.0).contains(&margin),
            "margin must be 18..144 points"
        );
        let font_size = config_f64_or(value, "font_size", ctx, 11.0)?;
        ensure!(
            (8.0..=24.0).contains(&font_size),
            "font_size must be 8..24 points"
        );
        let mut images = BTreeMap::new();
        if let Some(bindings) = value.get("images") {
            let bindings = bindings.as_object().ok_or_else(|| {
                anyhow::anyhow!(
                    "images must be an object mapping Markdown references to file sources"
                )
            })?;
            ensure!(
                bindings.len() <= 16,
                "at most 16 image bindings are allowed"
            );
            for (key, source) in bindings {
                ensure!(key.len() <= 2048, "image reference is too long");
                let parsed = if let Some(source) = source.as_str() {
                    let resolved = interpolate_ctx_bounded(source, ctx, 4096)?;
                    if resolved.starts_with("artifact://") {
                        ArtifactRef::validate_uri(&resolved)?;
                        FileSource::artifact_uri(resolved)
                    } else {
                        FileSource::path(resolved)
                    }
                } else {
                    FileSource::artifact(ArtifactRef::from_value(source)?)
                };
                images.insert(key.clone(), parsed);
            }
        }
        let path = string(value, "output_path", None, ctx, 4096)?;
        ensure!(!path.trim().is_empty(), "output_path must not be empty");
        let output_key = string(value, "output_key", Some("document"), ctx, 256)?;
        ensure!(!output_key.is_empty(), "output_key must not be empty");
        let language = string(value, "language", Some("und"), ctx, 63)?;
        parley::fontique::Language::parse(&language)
            .map_err(|_| anyhow::anyhow!("language must be a valid language tag"))?;
        let direction = match string(value, "direction", Some("auto"), ctx, 16)?.as_str() {
            "auto" => Direction::Auto,
            "ltr" => Direction::Ltr,
            "rtl" => Direction::Rtl,
            _ => anyhow::bail!("direction must be auto, ltr or rtl"),
        };
        Ok(Self {
            input,
            path: path.into(),
            output_key,
            overwrite: config_bool_or(value, "overwrite", ctx, false)?,
            width,
            height,
            margin: margin as f32,
            font_size: font_size as f32,
            title: string(value, "title", Some("IronFlow document"), ctx, 1024)?,
            author: string(value, "author", Some(""), ctx, 1024)?,
            language,
            direction,
            images,
        })
    }
}

fn string(
    value: &Value,
    key: &str,
    default: Option<&str>,
    ctx: &Context,
    maximum: usize,
) -> Result<String> {
    let text = match value.get(key) {
        Some(value) => value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{key} must be a string"))?,
        None => default.ok_or_else(|| anyhow::anyhow!("{key} is required"))?,
    };
    let text = interpolate_ctx_bounded(text, ctx, maximum)?;
    valid_text(&text)?;
    Ok(text)
}
