use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct ReadFileNode;

#[async_trait]
impl Node for ReadFileNode {
    fn node_type(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read file contents (text or binary as base64)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("read_file requires 'path' parameter"))?;

        let path = interpolate_ctx(path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("file");
        let encoding = config
            .get("encoding")
            .and_then(|v| v.as_str())
            .unwrap_or("text");

        // Pre-flight size guard: fail before allocating a huge buffer.
        let max_bytes = crate::util::limits::max_file_bytes();
        if let Ok(meta) = tokio::fs::metadata(&path).await
            && meta.len() > max_bytes
        {
            anyhow::bail!(
                "read_file: '{}' is {} bytes, exceeds limit {} (set IRONFLOW_MAX_FILE_BYTES to raise)",
                path,
                meta.len(),
                max_bytes
            );
        }

        let content = match encoding {
            "base64" => {
                let bytes = tokio::fs::read(&path).await?;
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            }
            "text" => tokio::fs::read_to_string(&path).await?,
            other => anyhow::bail!(
                "read_file: unsupported encoding '{}'. Must be 'text' or 'base64'.",
                other
            ),
        };

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_content", output_key),
            serde_json::Value::String(content),
        );
        output.insert(
            format!("{}_path", output_key),
            serde_json::Value::String(path),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct WriteFileNode;

#[async_trait]
impl Node for WriteFileNode {
    fn node_type(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file (text or binary from base64)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("write_file requires 'path' parameter"))?;

        let path = interpolate_ctx(path, ctx);
        let encoding = config
            .get("encoding")
            .and_then(|v| v.as_str())
            .unwrap_or("text");
        let append = config
            .get("append")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Resolve content bytes: from `content` string or `source_key` context value
        let bytes: Vec<u8> =
            if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
                let val = ctx
                    .get(source_key)
                    .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
                let s = val
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Value at '{}' must be a string", source_key))?;
                match encoding {
                    "base64" => base64::engine::general_purpose::STANDARD
                        .decode(s)
                        .map_err(|e| {
                            anyhow::anyhow!("Failed to decode base64 from '{}': {}", source_key, e)
                        })?,
                    "text" => s.as_bytes().to_vec(),
                    other => anyhow::bail!(
                        "write_file: unsupported encoding '{}'. Must be 'text' or 'base64'.",
                        other
                    ),
                }
            } else {
                let content = config.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let content = interpolate_ctx(content, ctx);
                content.into_bytes()
            };

        let max_bytes = crate::util::limits::max_file_bytes();
        if bytes.len() as u64 > max_bytes {
            anyhow::bail!(
                "write_file: payload {} bytes exceeds limit {} (set IRONFLOW_MAX_FILE_BYTES to raise)",
                bytes.len(),
                max_bytes
            );
        }

        if append {
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await?;
            file.write_all(&bytes).await?;
        } else {
            tokio::fs::write(&path, &bytes).await?;
        }

        let mut output = NodeOutput::new();
        output.insert(
            "write_file_path".to_string(),
            serde_json::Value::String(path),
        );
        output.insert(
            "write_file_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct CopyFileNode;

#[async_trait]
impl Node for CopyFileNode {
    fn node_type(&self) -> &str {
        "copy_file"
    }

    fn description(&self) -> &str {
        "Copy a file to a new location"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = config
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("copy_file requires 'source' parameter"))?;

        let destination = config
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("copy_file requires 'destination' parameter"))?;

        let source = interpolate_ctx(source, ctx);
        let destination = interpolate_ctx(destination, ctx);

        tokio::fs::copy(&source, &destination).await?;

        let mut output = NodeOutput::new();
        output.insert(
            "copy_file_source".to_string(),
            serde_json::Value::String(source),
        );
        output.insert(
            "copy_file_destination".to_string(),
            serde_json::Value::String(destination),
        );
        output.insert(
            "copy_file_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct MoveFileNode;

#[async_trait]
impl Node for MoveFileNode {
    fn node_type(&self) -> &str {
        "move_file"
    }

    fn description(&self) -> &str {
        "Move a file to a new location"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = config
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("move_file requires 'source' parameter"))?;

        let destination = config
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("move_file requires 'destination' parameter"))?;

        let source = interpolate_ctx(source, ctx);
        let destination = interpolate_ctx(destination, ctx);

        tokio::fs::rename(&source, &destination).await?;

        let mut output = NodeOutput::new();
        output.insert(
            "move_file_source".to_string(),
            serde_json::Value::String(source),
        );
        output.insert(
            "move_file_destination".to_string(),
            serde_json::Value::String(destination),
        );
        output.insert(
            "move_file_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct DeleteFileNode;

#[async_trait]
impl Node for DeleteFileNode {
    fn node_type(&self) -> &str {
        "delete_file"
    }

    fn description(&self) -> &str {
        "Delete a file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("delete_file requires 'path' parameter"))?;

        let path = interpolate_ctx(path, ctx);

        tokio::fs::remove_file(&path).await?;

        let mut output = NodeOutput::new();
        output.insert(
            "delete_file_path".to_string(),
            serde_json::Value::String(path),
        );
        output.insert(
            "delete_file_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct ListDirectoryNode;

#[async_trait]
impl Node for ListDirectoryNode {
    fn node_type(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files in a directory"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("list_directory requires 'path' parameter"))?;

        let path = interpolate_ctx(path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("files");

        let recursive = config
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let entries = list_dir_entries(&path, recursive).await?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(entries));
        Ok(output)
    }
}

/// Recursively list directory entries.
fn list_dir_entries(
    path: &str,
    recursive: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>
{
    Box::pin(async move {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path).await?;

        while let Some(entry) = dir.next_entry().await? {
            let file_type = entry.file_type().await?;
            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = entry.path().to_string_lossy().to_string();

            let type_str = if file_type.is_file() {
                "file"
            } else {
                "directory"
            };

            entries.push(serde_json::json!({
                "name": name,
                "type": type_str,
                "path": entry_path,
            }));

            if recursive && file_type.is_dir() {
                let sub_entries = list_dir_entries(&entry_path, true).await?;
                entries.extend(sub_entries);
            }
        }

        Ok(entries)
    })
}

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
        let files_count = tokio::task::spawn_blocking(move || {
            create_zip_archive(&source_clone, &zip_path_clone, include_root, compression)
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
        let entries = tokio::task::spawn_blocking(move || list_zip_entries(&zip_path_clone))
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
        let extracted = tokio::task::spawn_blocking(move || {
            extract_zip_archive(&zip_path_clone, &destination_clone, overwrite)
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

fn zip_collect_entries(source: &Path, include_root: bool) -> Result<Vec<(PathBuf, String)>> {
    let mut entries = Vec::new();

    if source.is_file() {
        let file_name = source.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            anyhow::anyhow!("zip_create: source file path has no valid file name")
        })?;

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

    walk_dir_for_zip(source, root_prefix.as_deref().unwrap_or(""), &mut entries)?;
    Ok(entries)
}

fn walk_dir_for_zip(
    directory: &Path,
    prefix: &str,
    entries: &mut Vec<(PathBuf, String)>,
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
            walk_dir_for_zip(&path, &child_prefix, entries)?;
        } else if path.is_file() {
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
) -> Result<usize> {
    let source = Path::new(source);
    if !source.exists() {
        anyhow::bail!("zip_create: source '{}' does not exist", source.display());
    }

    if let Some(parent) = Path::new(zip_path).parent() {
        fs::create_dir_all(parent)?;
    }

    let entries = zip_collect_entries(source, include_root)?;

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

fn list_zip_entries(zip_path: &str) -> Result<Vec<serde_json::Value>> {
    let file = File::open(zip_path)
        .map_err(|e| anyhow::anyhow!("zip_list: failed to open '{}': {}", zip_path, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        anyhow::anyhow!("zip_list: '{}' is not a valid zip archive: {}", zip_path, e)
    })?;

    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
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

fn extract_zip_archive(zip_path: &str, destination: &str, overwrite: bool) -> Result<Vec<String>> {
    let file = File::open(zip_path)
        .map_err(|e| anyhow::anyhow!("zip_extract: failed to open '{}': {}", zip_path, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        anyhow::anyhow!(
            "zip_extract: '{}' is not a valid zip archive: {}",
            zip_path,
            e
        )
    })?;

    let destination = Path::new(destination);
    fs::create_dir_all(destination)?;
    let destination = destination.canonicalize()?;
    let mut extracted = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
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

fn validate_zip_entry_name(name: &str) -> Result<String> {
    if name.is_empty() {
        anyhow::bail!("zip_extract: empty entry name in archive");
    }
    if name.contains('\\') {
        anyhow::bail!(
            "zip_extract: archive entry uses unsupported path separator: {}",
            name
        );
    }
    if Path::new(name).is_absolute() {
        anyhow::bail!("zip_extract: absolute path in archive entry: {}", name);
    }
    for component in Path::new(name).components() {
        if matches!(component, Component::ParentDir) {
            anyhow::bail!(
                "zip_extract: path traversal attempt in archive entry: {}",
                name
            );
        }
    }

    Ok(name.to_string())
}
