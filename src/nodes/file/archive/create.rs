use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::super::helpers::ZipLimits;
use super::copy::copy_with_control;
use super::rooted::RootedDir;
use crate::util::bounded_read::open_regular_file;
use crate::util::execution::ExecutionControl;

struct SourceEntry {
    path: PathBuf,
    archive_name: String,
}

struct PendingDirectory {
    path: PathBuf,
    prefix: String,
    depth: usize,
}

pub(super) fn parse_zip_compression(value: &str) -> Result<zip::CompressionMethod> {
    match value {
        "stored" => Ok(zip::CompressionMethod::Stored),
        "deflated" | "deflate" => Ok(zip::CompressionMethod::Deflated),
        other => anyhow::bail!(
            "zip_create: unsupported compression '{}'. Use 'stored' or 'deflated'.",
            other
        ),
    }
}

pub(super) fn create_zip_archive(
    source: &str,
    zip_path: &str,
    include_root: bool,
    compression: zip::CompressionMethod,
    limits: ZipLimits,
    execution: &ExecutionControl,
) -> Result<usize> {
    execution.checkpoint()?;
    let entries = collect_entries(Path::new(source), include_root, limits, execution)?;
    let output = Path::new(zip_path);
    let parent = output.parent().unwrap_or_else(|| Path::new(""));
    let leaf = output
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("zip_create: output path has no file name"))?;
    let root = RootedDir::prepare(parent, "zip_create", execution)?;
    let mut staged = root.stage_file(Path::new(leaf), true, execution)?;
    let files_count = entries.len();
    let mut actual_total = 0u64;

    {
        let mut writer = zip::ZipWriter::new(staged.writer());
        for entry in entries {
            execution.checkpoint()?;
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(compression)
                .unix_permissions(0o600);
            writer.start_file(&entry.archive_name, options)?;
            let mut file = open_regular_file(&entry.path, "zip_create")?;
            let remaining = limits
                .max_total_uncompressed_bytes
                .saturating_sub(actual_total);
            let copied =
                copy_with_control(&mut file, &mut writer, execution, remaining, "zip_create")?;
            actual_total = actual_total.saturating_add(copied);
        }
        execution.checkpoint()?;
        writer.finish()?;
    }

    execution.checkpoint()?;
    staged.commit()?;
    Ok(files_count)
}

fn collect_entries(
    source: &Path,
    include_root: bool,
    limits: ZipLimits,
    execution: &ExecutionControl,
) -> Result<Vec<SourceEntry>> {
    execution.checkpoint()?;
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("zip_create: failed to inspect '{}'", source.display()))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        anyhow::bail!(
            "zip_create: source '{}' is a symlink; symlinks are not followed",
            source.display()
        );
    }
    if file_type.is_file() {
        validate_source_capacity(1, metadata.len(), limits)?;
        let archive_name = source_name(source)?;
        return Ok(vec![SourceEntry {
            path: source.to_path_buf(),
            archive_name,
        }]);
    }
    if !file_type.is_dir() {
        anyhow::bail!(
            "zip_create: source '{}' is not a regular file or directory",
            source.display()
        );
    }

    let prefix = if include_root {
        source_name(source)?
    } else {
        String::new()
    };
    collect_directory_entries(source, prefix, limits, execution)
}

fn collect_directory_entries(
    source: &Path,
    prefix: String,
    limits: ZipLimits,
    execution: &ExecutionControl,
) -> Result<Vec<SourceEntry>> {
    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    let mut visited = 0usize;
    let mut pending = vec![PendingDirectory {
        path: source.to_path_buf(),
        prefix,
        depth: 0,
    }];

    while let Some(directory) = pending.pop() {
        execution.checkpoint()?;
        let mut children = Vec::new();
        for child in fs::read_dir(&directory.path).with_context(|| {
            format!(
                "zip_create: failed to read directory '{}'",
                directory.path.display()
            )
        })? {
            execution.checkpoint()?;
            visited = visited.saturating_add(1);
            if visited > limits.max_entries {
                anyhow::bail!(
                    "zip_create: source entry count exceeds limit {} (files and directories both consume traversal work)",
                    limits.max_entries
                );
            }
            children.push(child?);
        }
        children.sort_by_key(|entry| entry.file_name());

        let mut child_directories = Vec::new();
        for child in children {
            execution.checkpoint()?;
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("zip_create: non-UTF-8 file name"))?;
            validate_source_component(&name)?;
            let archive_name = join_archive_name(&directory.prefix, &name);
            let path = child.path();
            let metadata = fs::symlink_metadata(&path)?;
            let file_type = metadata.file_type();

            if file_type.is_symlink() {
                anyhow::bail!(
                    "zip_create: source entry '{}' is a symlink; symlinks are not followed",
                    path.display()
                );
            }
            if file_type.is_dir() {
                let depth = directory.depth.saturating_add(1);
                if depth > limits.max_depth {
                    anyhow::bail!(
                        "zip_create: source directory '{}' has depth {}, exceeds limit {}",
                        path.display(),
                        depth,
                        limits.max_depth
                    );
                }
                child_directories.push(PendingDirectory {
                    path,
                    prefix: archive_name,
                    depth,
                });
            } else if file_type.is_file() {
                total_bytes = total_bytes.saturating_add(metadata.len());
                validate_source_capacity(visited, total_bytes, limits)?;
                files.push(SourceEntry { path, archive_name });
            } else {
                anyhow::bail!(
                    "zip_create: source entry '{}' is not a regular file or directory",
                    path.display()
                );
            }
        }
        pending.extend(child_directories.into_iter().rev());
    }
    Ok(files)
}

fn validate_source_capacity(entry_count: usize, total_bytes: u64, limits: ZipLimits) -> Result<()> {
    if entry_count > limits.max_entries {
        anyhow::bail!(
            "zip_create: source entry count exceeds limit {}",
            limits.max_entries
        );
    }
    if total_bytes > limits.max_total_uncompressed_bytes {
        anyhow::bail!(
            "zip_create: total source bytes exceed uncompressed limit {}",
            limits.max_total_uncompressed_bytes
        );
    }
    Ok(())
}

fn source_name(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("zip_create: source path has no valid UTF-8 file name"))?;
    validate_source_component(name)?;
    Ok(name.to_string())
}

fn validate_source_component(name: &str) -> Result<()> {
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        anyhow::bail!("zip_create: source contains a non-portable file name: {name}");
    }
    Ok(())
}

fn join_archive_name(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}
