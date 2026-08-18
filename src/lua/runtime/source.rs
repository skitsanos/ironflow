//! Bounded flow-source validation and file reads.

use std::fs::File;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};

use crate::util::execution::ExecutionControl;

const READ_CHUNK_BYTES: usize = 16 * 1024;
const LIMIT_ENV: &str = "IRONFLOW_MAX_FLOW_SOURCE_BYTES";

pub(super) fn validate(source: &str, max_bytes: u64) -> Result<()> {
    if source.len() as u64 > max_bytes {
        anyhow::bail!(
            "flow source exceeds the {max_bytes}-byte limit (raise {LIMIT_ENV} to allow it)"
        );
    }
    Ok(())
}

pub(super) fn read_file(
    path: &str,
    max_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<String> {
    checkpoint(execution)?;
    let path = Path::new(path);
    let mut file = open_regular_file(path)?;
    let metadata = file
        .metadata()
        .with_context(|| format!("failed to inspect flow file '{}'", path.display()))?;

    if !metadata.file_type().is_file() {
        anyhow::bail!("flow source must be a regular file");
    }
    if metadata.len() > max_bytes {
        anyhow::bail!(
            "flow source is {} bytes, exceeding the {max_bytes}-byte limit (raise {LIMIT_ENV} to allow it)",
            metadata.len()
        );
    }

    let bytes = read_capped(&mut file, max_bytes, execution)
        .with_context(|| format!("failed to read flow file '{}'", path.display()))?;
    String::from_utf8(bytes).context("flow source is not valid UTF-8")
}

fn read_capped(
    reader: &mut File,
    max_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; READ_CHUNK_BYTES];

    loop {
        checkpoint(execution)?;
        let count = reader.read(&mut chunk)?;
        checkpoint(execution)?;
        if count == 0 {
            return Ok(bytes);
        }

        let remaining = max_bytes.saturating_sub(bytes.len() as u64);
        if count as u64 > remaining {
            anyhow::bail!(
                "flow source exceeds the {max_bytes}-byte limit (raise {LIMIT_ENV} to allow it)"
            );
        }

        bytes
            .try_reserve(count)
            .context("failed to allocate memory for flow source")?;
        bytes.extend_from_slice(&chunk[..count]);
    }
}

#[cfg(unix)]
fn open_regular_file(path: &Path) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    // O_NONBLOCK makes opening a FIFO or device return promptly. We then use
    // fstat through File::metadata and reject anything except a regular file,
    // so replacing the path between a separate metadata check and open cannot
    // turn this bounded read into a blocking special-file read.
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .with_context(|| format!("failed to open flow file '{}'", path.display()))
}

#[cfg(not(unix))]
fn open_regular_file(path: &Path) -> Result<File> {
    // Platforms without O_NONBLOCK get a best-effort preflight plus the
    // authoritative opened-handle metadata check in read_file. A replacement
    // can still occur between these two operations on those platforms.
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("failed to inspect flow file '{}'", path.display()))?;
    if !metadata.file_type().is_file() {
        anyhow::bail!("flow source must be a regular file");
    }
    File::open(path).with_context(|| format!("failed to open flow file '{}'", path.display()))
}

fn checkpoint(execution: Option<&ExecutionControl>) -> Result<()> {
    if let Some(execution) = execution {
        execution.checkpoint()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_source_limit_is_inclusive() {
        validate("1234", 4).unwrap();
        let error = validate("12345", 4).unwrap_err();
        assert!(error.to_string().contains(LIMIT_ENV), "{error:#}");
    }

    #[test]
    fn regular_file_read_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flow.lua");
        std::fs::write(&path, b"12345").unwrap();

        let error = read_file(path.to_str().unwrap(), 4, None).unwrap_err();
        assert!(format!("{error:#}").contains("4-byte limit"), "{error:#}");
    }

    #[test]
    fn regular_file_at_limit_is_accepted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flow.lua");
        std::fs::write(&path, b"1234").unwrap();

        assert_eq!(read_file(path.to_str().unwrap(), 4, None).unwrap(), "1234");
    }

    #[cfg(unix)]
    #[test]
    fn fifo_is_rejected_without_waiting_for_a_writer() {
        use std::os::unix::ffi::OsStrExt;
        use std::sync::mpsc;
        use std::time::Duration;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flow.lua");
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        let result = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
        assert_eq!(
            result,
            0,
            "mkfifo failed: {}",
            std::io::Error::last_os_error()
        );

        let (sender, receiver) = mpsc::channel();
        let reader_path = path.clone();
        let reader = std::thread::spawn(move || {
            let outcome = read_file(reader_path.to_str().unwrap(), 1024, None);
            let _ = sender.send(outcome);
        });

        let outcome = match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(outcome) => outcome,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Release a regressed blocking FIFO open before failing, so
                // the test does not leave a stuck worker behind.
                drop(OpenOptions::new().write(true).open(&path));
                let _ = receiver.recv_timeout(Duration::from_secs(1));
                reader.join().unwrap();
                panic!("opening a FIFO as flow source blocked waiting for a writer");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                reader.join().unwrap();
                panic!("FIFO reader worker disconnected without a result");
            }
        };
        reader.join().unwrap();
        let error = outcome.unwrap_err();
        assert!(error.to_string().contains("regular file"), "{error:#}");
    }
}
