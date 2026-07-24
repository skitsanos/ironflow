use std::fs::{self, File};
use std::io;
use std::path::Path;

use anyhow::Result;

use super::super::helpers::{ZipLimits, validate_zip_entry_name};

pub(super) fn list_zip_entries(
    zip_path: &str,
    limits: ZipLimits,
) -> Result<Vec<serde_json::Value>> {
    let file = open_zip_file(zip_path, "zip_list")?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        anyhow::anyhow!("zip_list: '{}' is not a valid zip archive: {}", zip_path, e)
    })?;
    validate_entry_count("zip_list", archive.len(), limits)?;

    let mut entries = Vec::new();
    let mut total_uncompressed = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        add_uncompressed_size("zip_list", &mut total_uncompressed, entry.size(), limits)?;
        let name = entry.name().to_string();
        entries.push(serde_json::json!({
            "name": name,
            "is_directory": name.ends_with('/'),
            "size": entry.size(),
            "compressed_size": entry.compressed_size(),
            "crc32": entry.crc32(),
            "method": format!("{:?}", entry.compression()),
        }));
    }

    Ok(entries)
}

pub(super) fn extract_zip_archive(
    zip_path: &str,
    destination: &str,
    overwrite: bool,
    limits: ZipLimits,
) -> Result<Vec<String>> {
    let file = open_zip_file(zip_path, "zip_extract")?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        anyhow::anyhow!(
            "zip_extract: '{}' is not a valid zip archive: {}",
            zip_path,
            e
        )
    })?;
    validate_entry_count("zip_extract", archive.len(), limits)?;

    let destination = prepare_destination(destination)?;
    let mut extracted = Vec::new();
    let mut total_uncompressed = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        add_uncompressed_size("zip_extract", &mut total_uncompressed, entry.size(), limits)?;
        let raw_name = entry.name().to_string();
        let out_path = safe_output_path(&destination, &raw_name)?;

        if raw_name.ends_with('/') {
            fs::create_dir_all(&out_path)?;
        } else {
            write_entry(&mut entry, &out_path, overwrite)?;
        }

        extracted.push(raw_name);
    }

    Ok(extracted)
}

fn open_zip_file(zip_path: &str, operation: &str) -> Result<File> {
    File::open(zip_path)
        .map_err(|e| anyhow::anyhow!("{}: failed to open '{}': {}", operation, zip_path, e))
}

fn validate_entry_count(operation: &str, count: usize, limits: ZipLimits) -> Result<()> {
    if count > limits.max_entries {
        anyhow::bail!(
            "{}: archive has {} entries, exceeds limit {}",
            operation,
            count,
            limits.max_entries
        );
    }
    Ok(())
}

fn add_uncompressed_size(
    operation: &str,
    total: &mut u64,
    size: u64,
    limits: ZipLimits,
) -> Result<()> {
    *total = total.saturating_add(size);
    if *total > limits.max_total_uncompressed_bytes {
        anyhow::bail!(
            "{}: total uncompressed bytes exceed limit {}",
            operation,
            limits.max_total_uncompressed_bytes
        );
    }
    Ok(())
}

fn prepare_destination(destination: &str) -> Result<std::path::PathBuf> {
    let destination = Path::new(destination);
    fs::create_dir_all(destination)?;
    Ok(destination.canonicalize()?)
}

fn safe_output_path(destination: &Path, raw_name: &str) -> Result<std::path::PathBuf> {
    let safe_name = validate_zip_entry_name(raw_name)?;
    let out_path = destination.join(safe_name.replace('\\', "/"));
    if !out_path.starts_with(destination) {
        anyhow::bail!("zip_extract: unsafe path in archive: {}", raw_name);
    }
    Ok(out_path)
}

fn write_entry(
    entry: &mut zip::read::ZipFile<'_, File>,
    out_path: &Path,
    overwrite: bool,
) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if !overwrite && out_path.exists() {
        anyhow::bail!(
            "zip_extract: destination file already exists and overwrite=false: {}",
            out_path.display()
        );
    }

    let mut output_file = File::create(out_path).map_err(|e| {
        anyhow::anyhow!("zip_extract: cannot create '{}': {}", out_path.display(), e)
    })?;
    io::copy(entry, &mut output_file)?;
    Ok(())
}
