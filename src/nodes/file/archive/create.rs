use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::super::helpers::ZipLimits;

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
) -> Result<usize> {
    let source = Path::new(source);
    if !source.exists() {
        anyhow::bail!("zip_create: source '{}' does not exist", source.display());
    }

    if let Some(parent) = Path::new(zip_path).parent() {
        fs::create_dir_all(parent)?;
    }

    let entries = collect_entries(source, include_root, limits)?;
    let zip_file = File::create(zip_path)
        .map_err(|e| anyhow::anyhow!("zip_create: cannot create '{}': {}", zip_path, e))?;

    let mut writer = zip::ZipWriter::new(zip_file);
    let files_count = entries.len();

    for (path, name) in entries {
        let options = zip::write::SimpleFileOptions::default().compression_method(compression);
        writer.start_file(name, options)?;
        let mut file = File::open(&path).map_err(|e| {
            anyhow::anyhow!("zip_create: failed to open '{}': {}", path.display(), e)
        })?;
        io::copy(&mut file, &mut writer)?;
    }

    writer.finish()?;
    Ok(files_count)
}

fn collect_entries(
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

    let root_prefix = include_root
        .then(|| source.file_name().and_then(|n| n.to_str()))
        .flatten();
    collect_directory_entries(
        source,
        root_prefix.unwrap_or(""),
        &mut entries,
        &mut total_bytes,
        limits,
    )?;
    Ok(entries)
}

fn collect_directory_entries(
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
            collect_directory_entries(&path, &child_prefix, entries, total_bytes, limits)?;
        } else if path.is_file() {
            validate_file_capacity(entries.len(), &path, total_bytes, limits)?;
            entries.push((path, child_prefix));
        }
    }

    Ok(())
}

fn validate_file_capacity(
    entry_count: usize,
    path: &Path,
    total_bytes: &mut u64,
    limits: ZipLimits,
) -> Result<()> {
    if entry_count >= limits.max_entries {
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
    Ok(())
}
