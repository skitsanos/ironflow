//! Size-bounded file and stream reads.
//!
//! Node input frequently designates a file path (or a zip entry) whose contents
//! are read fully into memory. Trusting `metadata().len()` is not enough: a zip
//! bomb's outer file is tiny while an entry inflates to gigabytes, and special
//! files such as `/dev/zero` report length 0 yet stream forever. These helpers
//! bound the actual number of bytes read, so oversized input becomes an ordinary
//! node error instead of an allocation abort that kills the whole process.

use std::io::Read;
use std::path::Path;

use anyhow::Result;

/// Read a reader into memory, erroring if it yields more than `max_bytes`.
/// Never buffers more than `max_bytes + 1` bytes.
pub fn read_capped<R: Read>(mut reader: R, max_bytes: u64, what: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let read = (&mut reader)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut buf)?;
    if read as u64 > max_bytes {
        anyhow::bail!(
            "{what}: input exceeds the {max_bytes}-byte limit (raise the relevant IRONFLOW_MAX_* setting)"
        );
    }
    Ok(buf)
}

/// Read a reader into a UTF-8 string, bounded by `max_bytes`.
pub fn read_to_string_capped<R: Read>(reader: R, max_bytes: u64, what: &str) -> Result<String> {
    let bytes = read_capped(reader, max_bytes, what)?;
    Ok(String::from_utf8(bytes)?)
}

/// Read a file into memory, bounded by `max_bytes`. A fast `metadata` pre-flight
/// rejects oversized regular files before opening; the bounded read then catches
/// special files whose reported length is unreliable.
pub fn read_file_capped(path: &Path, max_bytes: u64, what: &str) -> Result<Vec<u8>> {
    if let Ok(meta) = std::fs::metadata(path)
        && meta.len() > max_bytes
    {
        anyhow::bail!(
            "{what}: '{}' is {} bytes, exceeds the {} byte limit (raise the relevant IRONFLOW_MAX_* setting)",
            path.display(),
            meta.len(),
            max_bytes
        );
    }
    let file = std::fs::File::open(path)
        .map_err(|e| anyhow::anyhow!("{what}: failed to read '{}': {e}", path.display()))?;
    read_capped(file, max_bytes, what)
}

/// Read a file into a UTF-8 string, bounded by `max_bytes`.
pub fn read_file_to_string_capped(path: &Path, max_bytes: u64, what: &str) -> Result<String> {
    let bytes = read_file_capped(path, max_bytes, what)?;
    Ok(String::from_utf8(bytes)?)
}

/// Async variant of [`read_file_capped`].
pub async fn read_file_capped_async(path: &Path, max_bytes: u64, what: &str) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;

    if let Ok(meta) = tokio::fs::metadata(path).await
        && meta.len() > max_bytes
    {
        anyhow::bail!(
            "{what}: '{}' is {} bytes, exceeds the {} byte limit (raise the relevant IRONFLOW_MAX_* setting)",
            path.display(),
            meta.len(),
            max_bytes
        );
    }
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| anyhow::anyhow!("{what}: failed to read '{}': {e}", path.display()))?;
    let mut buf = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut buf)
        .await?;
    if buf.len() as u64 > max_bytes {
        anyhow::bail!(
            "{what}: '{}' exceeds the {} byte limit (raise the relevant IRONFLOW_MAX_* setting)",
            path.display(),
            max_bytes
        );
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_capped_accepts_within_limit() {
        let data = b"hello world";
        let out = read_capped(&data[..], 1024, "test").unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn read_capped_rejects_over_limit() {
        let data = vec![b'a'; 2048];
        let err = read_capped(&data[..], 1024, "test").unwrap_err();
        assert!(err.to_string().contains("exceeds"), "{err}");
    }

    #[test]
    fn read_capped_accepts_exactly_the_limit() {
        let data = vec![b'a'; 1024];
        let out = read_capped(&data[..], 1024, "test").unwrap();
        assert_eq!(out.len(), 1024);
    }
}
