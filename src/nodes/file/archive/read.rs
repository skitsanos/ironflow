use std::fs::File;
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
    let file = open_zip_file(zip_path, "zip_list")?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        anyhow::anyhow!(
            "zip_list: '{}' is not a valid ZIP archive: {}",
            zip_path,
            error
        )
    })?;
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

pub(super) fn open_zip_file(zip_path: &str, operation: &str) -> Result<File> {
    open_regular_file(Path::new(zip_path), operation)
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
