use anyhow::Result;
use std::path::{Path, PathBuf};

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
    pub(super) max_depth: usize,
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
        max_depth: config_u64(config, "max_depth", ctx)
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or_else(|| crate::util::limits::max_directory_depth() as usize),
        max_total_uncompressed_bytes: optional_u64(config, "max_total_uncompressed_bytes", ctx)
            .unwrap_or_else(crate::util::limits::max_zip_uncompressed_bytes),
    }
}

pub(super) fn validate_zip_entry_name(
    name: &str,
    is_directory: bool,
    max_depth: usize,
) -> Result<PathBuf> {
    if name.is_empty() || name.contains('\0') {
        anyhow::bail!("zip_extract: empty entry name in archive");
    }
    if name.contains('\\') {
        anyhow::bail!(
            "zip_extract: archive entry uses unsupported path separator: {}",
            name
        );
    }
    if Path::new(name).is_absolute() || name.starts_with('/') {
        anyhow::bail!("zip_extract: absolute path in archive entry: {}", name);
    }
    let normalized = if is_directory {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    let components = normalized.split('/').collect::<Vec<_>>();
    if components.is_empty() || components.iter().any(|component| component.is_empty()) {
        anyhow::bail!(
            "zip_extract: empty path component in archive entry: {}",
            name
        );
    }

    for component in &components {
        if *component == "." || *component == ".." {
            anyhow::bail!(
                "zip_extract: path traversal attempt in archive entry: {}",
                name
            );
        }
        validate_portable_zip_component(component, name)?;
    }

    let depth = components.len().saturating_sub(1);
    if depth > max_depth {
        anyhow::bail!(
            "zip_extract: entry '{}' has depth {}, exceeds limit {}",
            name,
            depth,
            max_depth
        );
    }

    Ok(components.iter().copied().collect())
}

fn validate_portable_zip_component(component: &str, entry_name: &str) -> Result<()> {
    if component.contains(':') || component.ends_with('.') || component.ends_with(' ') {
        anyhow::bail!(
            "zip_extract: archive entry is not a portable path: {}",
            entry_name
        );
    }

    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'));
    if reserved {
        anyhow::bail!(
            "zip_extract: archive entry uses a reserved path component: {}",
            entry_name
        );
    }
    Ok(())
}
