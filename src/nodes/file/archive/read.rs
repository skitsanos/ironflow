use std::fs::File;
use std::io::Seek;
use std::path::Path;

use anyhow::Result;

use super::super::helpers::ZipLimits;
use crate::util::bounded_read::open_regular_file;
use crate::util::execution::ExecutionControl;

pub(super) fn list_zip_entries(
    zip_path: &str,
    limits: ZipLimits,
    execution: &ExecutionControl,
) -> Result<Vec<serde_json::Value>> {
    execution.checkpoint()?;
    let mut archive = open_zip_archive(zip_path, "zip_list", limits, execution)?;
    validate_entry_count("zip_list", archive.len(), limits)?;

    let mut entries = Vec::with_capacity(archive.len());
    let mut total_uncompressed = 0u64;
    for index in 0..archive.len() {
        execution.checkpoint()?;
        let entry = archive.by_index(index)?;
        add_uncompressed_size("zip_list", &mut total_uncompressed, entry.size(), limits)?;
        entries.push(serde_json::json!({
            "name": entry.name(),
            "is_directory": entry.is_dir(),
            "size": entry.size(),
            "compressed_size": entry.compressed_size(),
            "crc32": entry.crc32(),
            "method": format!("{:?}", entry.compression()),
        }));
    }
    execution.checkpoint()?;
    Ok(entries)
}

pub(super) fn open_zip_archive(
    zip_path: &str,
    operation: &str,
    limits: ZipLimits,
    execution: &ExecutionControl,
) -> Result<zip::ZipArchive<File>> {
    let mut file = open_regular_file(Path::new(zip_path), operation)?;
    let entries = crate::util::zip_preflight::check(
        &mut file,
        Path::new(zip_path),
        operation,
        crate::util::zip_preflight::Limits {
            max_entries: limits.max_entries as u64,
            // Payload bytes have their own decompression limit. A metadata
            // budget must not become an unrelated compressed-file size limit.
            max_raw_bytes: u64::MAX,
            raw_limit_name: "archive size",
            max_metadata_bytes: limits.max_metadata_bytes,
            metadata_limit_name: "max_metadata_bytes / IRONFLOW_MAX_ZIP_METADATA_BYTES",
        },
        Some(execution),
    )?;
    execution.checkpoint()?;
    file.rewind()?;
    let archive = zip::ZipArchive::new(file).map_err(|error| {
        anyhow::anyhow!("{operation}: '{zip_path}' is not a valid ZIP archive: {error}")
    })?;
    execution.checkpoint()?;
    // Unicode path extra fields can alias otherwise distinct raw names.
    if archive.len() as u64 != entries {
        anyhow::bail!(
            "{operation}: duplicate archive names or entry count changed during ZIP parsing"
        );
    }
    Ok(archive)
}

pub(super) fn validate_entry_count(operation: &str, count: usize, limits: ZipLimits) -> Result<()> {
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

pub(super) fn add_uncompressed_size(
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
