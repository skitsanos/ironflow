use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result};
use percent_encoding::percent_decode_str;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TargetKind {
    Missing,
    File,
    Directory,
    Rejected,
}

#[derive(Debug)]
pub(super) struct DecodedPath {
    relative: PathBuf,
}

impl DecodedPath {
    pub(super) fn parse(raw: &str) -> Option<Self> {
        if !raw.starts_with('/') || raw.starts_with("//") || !valid_percent_encoding(raw) {
            return None;
        }
        let decoded = percent_decode_str(raw).decode_utf8().ok()?;
        if decoded.contains('\\') || decoded.contains('\0') {
            return None;
        }

        let mut relative = PathBuf::new();
        for segment in decoded.trim_start_matches('/').split('/') {
            if segment.is_empty() {
                continue;
            }
            if segment == "." || segment == ".." {
                return None;
            }
            let mut components = Path::new(segment).components();
            match (components.next(), components.next()) {
                (Some(Component::Normal(component)), None) => relative.push(component),
                _ => return None,
            }
        }
        Some(Self { relative })
    }

    pub(super) fn relative(&self) -> &Path {
        &self.relative
    }

    pub(super) fn is_extensionless(&self) -> bool {
        self.relative
            .file_name()
            .is_none_or(|name| Path::new(name).extension().is_none())
    }

    pub(super) fn is_reserved(&self) -> bool {
        self.relative
            .components()
            .next()
            .and_then(|component| match component {
                Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .is_some_and(|segment| {
                matches!(
                    segment,
                    "flows" | "runs" | "nodes" | "webhooks" | "health" | "metrics"
                )
            })
    }
}

pub(super) async fn validated_root(configured: &Path) -> Result<PathBuf> {
    if configured.as_os_str().is_empty() {
        anyhow::bail!("static.directory cannot be empty");
    }
    let metadata = tokio::fs::symlink_metadata(configured)
        .await
        .with_context(|| format!("static.directory '{}' is unavailable", configured.display()))?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "static.directory '{}' must not be a symlink",
            configured.display()
        );
    }
    if !metadata.is_dir() {
        anyhow::bail!(
            "static.directory '{}' must be a directory",
            configured.display()
        );
    }
    let root = tokio::fs::canonicalize(configured).await.with_context(|| {
        format!(
            "failed to resolve static.directory '{}'",
            configured.display()
        )
    })?;
    let _read_dir = tokio::fs::read_dir(&root).await.with_context(|| {
        format!(
            "static.directory '{}' is not readable",
            configured.display()
        )
    })?;
    Ok(root)
}

pub(super) fn validate_index_name(index: &str) -> Result<()> {
    let valid = !index.is_empty()
        && index.len() <= 255
        && index != "."
        && index != ".."
        && index
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if !valid {
        anyhow::bail!(
            "static.index must be a portable 1..=255 byte file name using letters, digits, '.', '_', or '-'"
        );
    }
    Ok(())
}

pub(super) async fn validated_index(root: &Path, index: &str, precompressed: bool) -> Result<()> {
    let candidate = root.join(index);
    let metadata = tokio::fs::symlink_metadata(&candidate)
        .await
        .with_context(|| format!("static.index '{index}' is unavailable in static.directory"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!("static.index '{index}' must be a regular non-symlink file");
    }
    if inspect(root, Path::new(index), precompressed).await != TargetKind::File {
        anyhow::bail!("static.index '{index}' is not safely confined to static.directory");
    }
    Ok(())
}

pub(super) async fn inspect(root: &Path, relative: &Path, precompressed: bool) -> TargetKind {
    let candidate = root.join(relative);
    let canonical = match tokio::fs::canonicalize(&candidate).await {
        Ok(path) if path.starts_with(root) => path,
        Ok(_) => return TargetKind::Rejected,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return TargetKind::Missing;
        }
        Err(_) => return TargetKind::Rejected,
    };
    let metadata = match tokio::fs::metadata(canonical).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return TargetKind::Missing;
        }
        Err(_) => return TargetKind::Rejected,
    };
    if metadata.is_dir() {
        return TargetKind::Directory;
    }
    if !metadata.is_file() {
        return TargetKind::Rejected;
    }
    if precompressed && !sidecars_are_confined(root, &candidate).await {
        return TargetKind::Rejected;
    }
    TargetKind::File
}

async fn sidecars_are_confined(root: &Path, candidate: &Path) -> bool {
    for suffix in [".br", ".gz"] {
        let sidecar = append_suffix(candidate, suffix);
        let metadata = match tokio::fs::symlink_metadata(&sidecar).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return false,
        };
        if !metadata.is_file() && !metadata.file_type().is_symlink() {
            return false;
        }
        let canonical = match tokio::fs::canonicalize(&sidecar).await {
            Ok(path) => path,
            Err(_) => return false,
        };
        if !canonical.starts_with(root) {
            return false;
        }
        if !tokio::fs::metadata(canonical)
            .await
            .is_ok_and(|metadata| metadata.is_file())
        {
            return false;
        }
    }
    true
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(suffix);
    PathBuf::from(value)
}

fn valid_percent_encoding(path: &str) -> bool {
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return false;
        }
        let Some(high) = hex(bytes[index + 1]) else {
            return false;
        };
        let Some(low) = hex(bytes[index + 2]) else {
            return false;
        };
        if matches!((high << 4) | low, b'/' | b'\\') {
            return false;
        }
        index += 3;
    }
    true
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
