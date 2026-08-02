use anyhow::Result;

use crate::artifacts::FileSource;
use crate::engine::types::Context;
use crate::util::file_source::parse_file_source;

pub(super) fn parse_sources(
    config: &serde_json::Value,
    ctx: &Context,
    maximum: u64,
) -> Result<Vec<FileSource>> {
    let files = match (config.get("files"), config.get("source_key")) {
        (Some(_), Some(_)) => {
            anyhow::bail!("pdf_merge accepts either 'files' or 'source_key', not both")
        }
        (Some(files), None) => files
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("pdf_merge: 'files' must be an array"))?,
        (None, Some(serde_json::Value::String(key))) => ctx
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("Key '{key}' not found in context"))?
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Context key '{key}' must be an array"))?,
        (None, Some(_)) => anyhow::bail!("pdf_merge: 'source_key' must be a string"),
        (None, None) => anyhow::bail!("pdf_merge requires either 'files' or 'source_key'"),
    };
    if files.is_empty() {
        anyhow::bail!("pdf_merge: input array must not be empty");
    }
    let count = u64::try_from(files.len()).unwrap_or(u64::MAX);
    if count > maximum {
        anyhow::bail!("pdf_merge: {count} inputs exceed IRONFLOW_MAX_PDF_MERGE_FILES ({maximum})");
    }
    let mut sources = Vec::new();
    sources.try_reserve_exact(files.len())?;
    for file in files {
        sources.push(parse_file_source(file, ctx, "pdf_merge").map_err(|error| {
            anyhow::anyhow!("pdf_merge: each input must be a path or artifact: {error}")
        })?);
    }
    Ok(sources)
}

pub(super) fn required_string<'a>(config: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    optional_string(config, key)?
        .ok_or_else(|| anyhow::anyhow!("pdf_merge requires '{key}' parameter"))
}

pub(super) fn optional_string<'a>(
    config: &'a serde_json::Value,
    key: &str,
) -> Result<Option<&'a str>> {
    match config.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(_) => anyhow::bail!("pdf_merge: '{key}' must be a string"),
    }
}
