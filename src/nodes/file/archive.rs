use anyhow::Result;
use async_trait::async_trait;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::helpers::{ZipLimits, validate_zip_entry_name, zip_limits};

pub struct ZipCreateNode;

#[async_trait]
impl Node for ZipCreateNode {
    fn node_type(&self) -> &str {
        "zip_create"
    }

    fn description(&self) -> &str {
        "Create a ZIP archive from a file or directory"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = config
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_create requires 'source' parameter"))?;

        let zip_path = config
            .get("zip_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_create requires 'zip_path' parameter"))?;

        let source = interpolate_ctx(source, ctx);
        let zip_path = interpolate_ctx(zip_path, ctx);
        let include_root = config
            .get("include_root")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let compression = parse_zip_compression(
            config
                .get("compression")
                .and_then(|v| v.as_str())
                .unwrap_or("deflated"),
        )?;

        let zip_path_clone = zip_path.clone();
        let source_clone = source.clone();
        let limits = zip_limits(config);
        let files_count = tokio::task::spawn_blocking(move || {
            create_zip_archive(
                &source_clone,
                &zip_path_clone,
                include_root,
                compression,
                limits,
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("zip_create: worker task failed: {}", e))??;

        let mut output = NodeOutput::new();
        output.insert(
            "zip_create_path".to_string(),
            serde_json::Value::String(zip_path),
        );
        output.insert(
            "zip_create_source".to_string(),
            serde_json::Value::String(source),
        );
        output.insert(
            "zip_create_files".to_string(),
            serde_json::Value::Number((files_count as u64).into()),
        );
        output.insert(
            "zip_create_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct ZipListNode;

#[async_trait]
impl Node for ZipListNode {
    fn node_type(&self) -> &str {
        "zip_list"
    }

    fn description(&self) -> &str {
        "List entries in a ZIP archive"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let zip_path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_list requires 'path' parameter"))?;

        let zip_path = interpolate_ctx(zip_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("zip_entries");

        let zip_path_clone = zip_path.clone();
        let limits = zip_limits(config);
        let entries =
            tokio::task::spawn_blocking(move || list_zip_entries(&zip_path_clone, limits))
                .await
                .map_err(|e| anyhow::anyhow!("zip_list: worker task failed: {}", e))??;

        let mut output = NodeOutput::new();
        let count = entries.len() as u64;
        output.insert(output_key.to_string(), serde_json::json!(entries));
        output.insert(
            format!("{output_key}_count"),
            serde_json::Value::Number(count.into()),
        );
        output.insert(
            "zip_list_path".to_string(),
            serde_json::Value::String(zip_path),
        );
        output.insert(
            "zip_list_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct ZipExtractNode;

#[async_trait]
impl Node for ZipExtractNode {
    fn node_type(&self) -> &str {
        "zip_extract"
    }

    fn description(&self) -> &str {
        "Extract a ZIP archive into a directory"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let zip_path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_extract requires 'path' parameter"))?;

        let destination = config
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_extract requires 'destination' parameter"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("extracted_files");

        let overwrite = config
            .get("overwrite")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let zip_path = interpolate_ctx(zip_path, ctx);
        let destination = interpolate_ctx(destination, ctx);

        let zip_path_clone = zip_path.clone();
        let destination_clone = destination.clone();
        let limits = zip_limits(config);
        let extracted = tokio::task::spawn_blocking(move || {
            extract_zip_archive(&zip_path_clone, &destination_clone, overwrite, limits)
        })
        .await
        .map_err(|e| anyhow::anyhow!("zip_extract: worker task failed: {}", e))??;

        let count = extracted.len() as u64;
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::json!(extracted.clone()));
        output.insert(
            format!("{output_key}_count"),
            serde_json::Value::Number(count.into()),
        );
        output.insert(
            "zip_extract_path".to_string(),
            serde_json::Value::String(zip_path),
        );
        output.insert(
            "zip_extract_destination".to_string(),
            serde_json::Value::String(destination),
        );
        output.insert(
            "zip_extract_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

fn parse_zip_compression(value: &str) -> Result<zip::CompressionMethod> {
    match value {
        "stored" => Ok(zip::CompressionMethod::Stored),
        "deflated" | "deflate" => Ok(zip::CompressionMethod::Deflated),
        other => anyhow::bail!(
            "zip_create: unsupported compression '{}'. Use 'stored' or 'deflated'.",
            other
        ),
    }
}

fn zip_collect_entries(
    source: &Path,
    include_root: bool,
    limits: ZipLimits,
) -> Result<Vec<(PathBuf, String)>> {
    let mut entries = Vec::new();
    let mut total_bytes = 0u64;

    if source.is_file() {
        let file_name = source.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            anyhow::anyhow!("zip_create: source file path has no valid file name")
        })?;

        total_bytes = source.metadata()?.len();
        if total_bytes > limits.max_total_uncompressed_bytes {
            anyhow::bail!(
                "zip_create: source file is {} bytes, exceeds uncompressed limit {}",
                total_bytes,
                limits.max_total_uncompressed_bytes
            );
        }
        entries.push((source.to_path_buf(), file_name.replace('\\', "/")));
        return Ok(entries);
    }

    if !source.is_dir() {
        anyhow::bail!(
            "zip_create: source path '{}' is not a file or directory",
            source.display()
        );
    }

    let root_prefix = if include_root {
        source
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    } else {
        None
    };

    walk_dir_for_zip(
        source,
        root_prefix.as_deref().unwrap_or(""),
        &mut entries,
        &mut total_bytes,
        limits,
    )?;
    Ok(entries)
}

fn walk_dir_for_zip(
    directory: &Path,
    prefix: &str,
    entries: &mut Vec<(PathBuf, String)>,
    total_bytes: &mut u64,
    limits: ZipLimits,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("zip_create: non-utf8 file name"))?;
        let path = entry.path();

        let child_prefix = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };

        if path.is_dir() {
            walk_dir_for_zip(&path, &child_prefix, entries, total_bytes, limits)?;
        } else if path.is_file() {
            if entries.len() >= limits.max_entries {
                anyhow::bail!(
                    "zip_create: file count exceeds limit {} (set max_entries or IRONFLOW_MAX_ZIP_ENTRIES to raise)",
                    limits.max_entries
                );
            }
            *total_bytes = total_bytes.saturating_add(path.metadata()?.len());
            if *total_bytes > limits.max_total_uncompressed_bytes {
                anyhow::bail!(
                    "zip_create: total source bytes exceed limit {} (set max_total_uncompressed_bytes or IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES to raise)",
                    limits.max_total_uncompressed_bytes
                );
            }
            entries.push((path, child_prefix));
        }
    }

    Ok(())
}

fn create_zip_archive(
    source: &str,
    zip_path: &str,
    include_root: bool,
    compression: zip::CompressionMethod,
    limits: ZipLimits,
) -> Result<usize> {
    let source = Path::new(source);
    if !source.exists() {
        anyhow::bail!("zip_create: source '{}' does not exist", source.display());
    }

    if let Some(parent) = Path::new(zip_path).parent() {
        fs::create_dir_all(parent)?;
    }

    let entries = zip_collect_entries(source, include_root, limits)?;

    let zip_file = File::create(zip_path)
        .map_err(|e| anyhow::anyhow!("zip_create: cannot create '{}': {}", zip_path, e))?;

    let mut writer = zip::ZipWriter::new(zip_file);
    let method = compression;
    let files_count = entries.len();

    for (path, name) in entries {
        let options = zip::write::SimpleFileOptions::default().compression_method(method);
        writer.start_file(name, options)?;
        let mut file = File::open(&path).map_err(|e| {
            anyhow::anyhow!("zip_create: failed to open '{}': {}", path.display(), e)
        })?;
        io::copy(&mut file, &mut writer)?;
    }

    writer.finish()?;
    Ok(files_count)
}

fn list_zip_entries(zip_path: &str, limits: ZipLimits) -> Result<Vec<serde_json::Value>> {
    let file = File::open(zip_path)
        .map_err(|e| anyhow::anyhow!("zip_list: failed to open '{}': {}", zip_path, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        anyhow::anyhow!("zip_list: '{}' is not a valid zip archive: {}", zip_path, e)
    })?;

    if archive.len() > limits.max_entries {
        anyhow::bail!(
            "zip_list: archive has {} entries, exceeds limit {}",
            archive.len(),
            limits.max_entries
        );
    }

    let mut entries = Vec::new();
    let mut total_uncompressed = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > limits.max_total_uncompressed_bytes {
            anyhow::bail!(
                "zip_list: total uncompressed bytes exceed limit {}",
                limits.max_total_uncompressed_bytes
            );
        }
        let name = entry.name().to_string();
        let is_directory = name.ends_with('/');
        entries.push(serde_json::json!({
            "name": name,
            "is_directory": is_directory,
            "size": entry.size(),
            "compressed_size": entry.compressed_size(),
            "crc32": entry.crc32(),
            "method": format!("{:?}", entry.compression()),
        }));
    }

    Ok(entries)
}

fn extract_zip_archive(
    zip_path: &str,
    destination: &str,
    overwrite: bool,
    limits: ZipLimits,
) -> Result<Vec<String>> {
    let file = File::open(zip_path)
        .map_err(|e| anyhow::anyhow!("zip_extract: failed to open '{}': {}", zip_path, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        anyhow::anyhow!(
            "zip_extract: '{}' is not a valid zip archive: {}",
            zip_path,
            e
        )
    })?;

    if archive.len() > limits.max_entries {
        anyhow::bail!(
            "zip_extract: archive has {} entries, exceeds limit {}",
            archive.len(),
            limits.max_entries
        );
    }

    let destination = Path::new(destination);
    fs::create_dir_all(destination)?;
    let destination = destination.canonicalize()?;
    let mut extracted = Vec::new();
    let mut total_uncompressed = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > limits.max_total_uncompressed_bytes {
            anyhow::bail!(
                "zip_extract: total uncompressed bytes exceed limit {}",
                limits.max_total_uncompressed_bytes
            );
        }
        let raw_name = entry.name().to_string();
        let safe_name = validate_zip_entry_name(&raw_name)?;
        let out_path = destination.join(safe_name.replace('\\', "/"));

        if !out_path.starts_with(&destination) {
            anyhow::bail!("zip_extract: unsafe path in archive: {}", raw_name);
        }

        if raw_name.ends_with('/') {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            if !overwrite && out_path.exists() {
                anyhow::bail!(
                    "zip_extract: destination file already exists and overwrite=false: {}",
                    out_path.display()
                );
            }

            let mut output_file = File::create(&out_path).map_err(|e| {
                anyhow::anyhow!("zip_extract: cannot create '{}': {}", out_path.display(), e)
            })?;
            io::copy(&mut entry, &mut output_file)?;
        }

        extracted.push(raw_name);
    }

    Ok(extracted)
}
