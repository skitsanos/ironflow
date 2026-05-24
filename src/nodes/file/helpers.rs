use anyhow::Result;
use std::path::{Component, Path};

#[derive(Clone, Copy)]
pub(super) struct DirectoryListLimits {
    pub(super) max_entries: usize,
    pub(super) max_depth: usize,
}

#[derive(Clone, Copy)]
pub(super) struct ZipLimits {
    pub(super) max_entries: usize,
    pub(super) max_total_uncompressed_bytes: u64,
}

pub(super) fn optional_usize(config: &serde_json::Value, key: &str) -> Option<usize> {
    config
        .get(key)
        .and_then(|v| v.as_u64())
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v > 0)
}

pub(super) fn optional_u64(config: &serde_json::Value, key: &str) -> Option<u64> {
    config.get(key).and_then(|v| v.as_u64()).filter(|v| *v > 0)
}

pub(super) fn directory_list_limits(config: &serde_json::Value) -> DirectoryListLimits {
    DirectoryListLimits {
        max_entries: optional_usize(config, "max_entries")
            .unwrap_or_else(|| crate::util::limits::max_directory_entries() as usize),
        max_depth: config
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or_else(|| crate::util::limits::max_directory_depth() as usize),
    }
}

pub(super) fn zip_limits(config: &serde_json::Value) -> ZipLimits {
    ZipLimits {
        max_entries: optional_usize(config, "max_entries")
            .unwrap_or_else(|| crate::util::limits::max_zip_entries() as usize),
        max_total_uncompressed_bytes: optional_u64(config, "max_total_uncompressed_bytes")
            .unwrap_or_else(crate::util::limits::max_zip_uncompressed_bytes),
    }
}

pub(super) fn validate_zip_entry_name(name: &str) -> Result<String> {
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
