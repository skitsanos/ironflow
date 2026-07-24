use anyhow::Result;
use std::path::{Component, Path};

use crate::engine::types::Context;
use crate::util::node_config::config_u64;

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

pub(super) fn optional_usize(
    config: &serde_json::Value,
    key: &str,
    ctx: &Context,
) -> Option<usize> {
    config_u64(config, key, ctx)
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v > 0)
}

pub(super) fn optional_u64(config: &serde_json::Value, key: &str, ctx: &Context) -> Option<u64> {
    config_u64(config, key, ctx).filter(|v| *v > 0)
}

pub(super) fn directory_list_limits(
    config: &serde_json::Value,
    ctx: &Context,
) -> DirectoryListLimits {
    DirectoryListLimits {
        max_entries: optional_usize(config, "max_entries", ctx)
            .unwrap_or_else(|| crate::util::limits::max_directory_entries() as usize),
        max_depth: config_u64(config, "max_depth", ctx)
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or_else(|| crate::util::limits::max_directory_depth() as usize),
    }
}

pub(super) fn zip_limits(config: &serde_json::Value, ctx: &Context) -> ZipLimits {
    ZipLimits {
        max_entries: optional_usize(config, "max_entries", ctx)
            .unwrap_or_else(|| crate::util::limits::max_zip_entries() as usize),
        max_total_uncompressed_bytes: optional_u64(config, "max_total_uncompressed_bytes", ctx)
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
