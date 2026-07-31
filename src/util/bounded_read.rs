//! Size-bounded file and stream reads.
//!
//! Node input frequently designates a file path (or a zip entry) whose contents
//! are read fully into memory. Trusting `metadata().len()` is not enough: a zip
//! bomb's outer file is tiny while an entry inflates to gigabytes, and special
//! files such as `/dev/zero` report length 0 yet stream forever. These helpers
//! bound the actual number of bytes read, so oversized input becomes an ordinary
//! node error instead of an allocation abort that kills the whole process.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};

/// Open `path` and require the opened handle to refer to a regular file. Unix
/// additionally refuses to follow a final symlink.
///
/// The check deliberately uses the opened handle's metadata rather than a
/// path-level preflight, which would leave a time-of-check/time-of-use race.
/// On Unix, non-blocking open also makes FIFOs and devices safe to inspect:
/// opening a reader-only FIFO with no writer cannot pin an async executor or a
/// detached blocking worker before the regular-file check rejects it.
pub(crate) fn open_regular_file(path: &Path, what: &str) -> Result<File> {
    let file = open_file_no_follow(path).map_err(|error| {
        anyhow::anyhow!(
            "{what}: failed to read '{}': failed to open file: {error}",
            path.display()
        )
    })?;
    let metadata = file
        .metadata()
        .with_context(|| format!("{what}: failed to inspect '{}'", path.display()))?;
    if !metadata.is_file() {
        anyhow::bail!("{what}: '{}' is not a regular file", path.display());
    }
    Ok(file)
}

#[cfg(unix)]
fn open_file_no_follow(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(not(unix))]
fn open_file_no_follow(path: &Path) -> std::io::Result<File> {
    // There is no portable std API equivalent to O_NOFOLLOW. The authoritative
    // opened-handle regular-file check still prevents reading directories and
    // special files on these targets; Unix additionally gets race-free final
    // symlink rejection and non-blocking FIFO/device handling.
    File::open(path)
}

fn reserve_for_chunk(buf: &mut Vec<u8>, additional: usize, what: &str) -> Result<()> {
    buf.try_reserve_exact(additional)
        .with_context(|| format!("{what}: cannot reserve memory for the configured byte limit"))
}

/// Read a reader into memory, erroring if it yields more than `max_bytes`.
/// Never buffers more than `max_bytes + 1` bytes.
pub fn read_capped<R: Read>(mut reader: R, max_bytes: u64, what: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let read_limit = max_bytes.saturating_add(1);
    let mut chunk = [0_u8; 8 * 1024];
    while (buf.len() as u64) < read_limit {
        let remaining = read_limit.saturating_sub(buf.len() as u64);
        let request = chunk.len().min(remaining.try_into().unwrap_or(usize::MAX));
        let read = reader.read(&mut chunk[..request])?;
        if read == 0 {
            break;
        }
        reserve_for_chunk(&mut buf, read, what)?;
        buf.extend_from_slice(&chunk[..read]);
    }
    if buf.len() as u64 > max_bytes {
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

/// Read a regular file into memory, bounded by `max_bytes`.
pub fn read_file_capped(path: &Path, max_bytes: u64, what: &str) -> Result<Vec<u8>> {
    let file = open_regular_file(path, what)?;
    let len = file.metadata()?.len();
    if len > max_bytes {
        anyhow::bail!(
            "{what}: '{}' is {} bytes, exceeds the {} byte limit (raise the relevant IRONFLOW_MAX_* setting)",
            path.display(),
            len,
            max_bytes
        );
    }
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

    let file = open_regular_file(path, what)?;
    let len = file.metadata()?.len();
    if len > max_bytes {
        anyhow::bail!(
            "{what}: '{}' is {} bytes, exceeds the {} byte limit (raise the relevant IRONFLOW_MAX_* setting)",
            path.display(),
            len,
            max_bytes
        );
    }
    let mut file = tokio::fs::File::from_std(file);
    let mut buf = Vec::new();
    let read_limit = max_bytes.saturating_add(1);
    let mut chunk = [0_u8; 8 * 1024];
    while (buf.len() as u64) < read_limit {
        let remaining = read_limit.saturating_sub(buf.len() as u64);
        let request = chunk.len().min(remaining.try_into().unwrap_or(usize::MAX));
        let read = file.read(&mut chunk[..request]).await?;
        if read == 0 {
            break;
        }
        reserve_for_chunk(&mut buf, read, what)?;
        buf.extend_from_slice(&chunk[..read]);
    }
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

    #[test]
    fn regular_file_reads_preserve_exact_and_oversized_caps() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("payload.bin");
        std::fs::write(&path, b"exact").unwrap();

        assert_eq!(read_file_capped(&path, 5, "test").unwrap(), b"exact");
        let error = read_file_capped(&path, 4, "test").unwrap_err().to_string();
        assert!(error.contains("exceeds the 4 byte limit"), "{error}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fifo_without_a_writer_is_rejected_promptly() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audio.pipe");
        let path_c = CString::new(path.as_os_str().as_bytes()).unwrap();
        let result = unsafe { libc::mkfifo(path_c.as_ptr(), 0o600) };
        assert_eq!(
            result,
            0,
            "mkfifo failed: {}",
            std::io::Error::last_os_error()
        );

        let outcome = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            read_file_capped_async(&path, 1024, "test"),
        )
        .await
        .expect("FIFO open blocked despite O_NONBLOCK");
        let error = outcome.unwrap_err().to_string();
        assert!(error.contains("not a regular file"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn final_symlink_is_not_followed() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.bin");
        let link = directory.path().join("link.bin");
        std::fs::write(&target, b"secret").unwrap();
        symlink(&target, &link).unwrap();

        let error = read_file_capped(&link, 1024, "test")
            .unwrap_err()
            .to_string();
        assert!(error.contains("failed to open"), "{error}");
    }
}
