use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest as _, Sha256};

use crate::util::execution::ExecutionControl;

use super::filesystem;

const HASH_CHUNK_BYTES: usize = 16 * 1024;

pub(super) fn inspect_regular(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("artifact '{}' does not exist", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("artifact '{}' is not a regular file", path.display());
    }
    Ok(())
}

pub(super) fn verify_existing(
    path: &Path,
    expected_digest: &str,
    expected_size: u64,
    execution: &ExecutionControl,
) -> Result<()> {
    let mut file = crate::util::bounded_read::open_regular_file(path, "existing artifact")?;
    if file.metadata()?.len() != expected_size {
        bail!("existing content-addressed artifact has an unexpected size");
    }
    let digest = hash_reader(&mut file, expected_size, execution)?;
    if digest != expected_digest {
        bail!("existing content-addressed artifact failed digest verification");
    }
    filesystem::harden_file(&file)
}

pub(super) fn hash_reader(
    reader: &mut impl Read,
    expected_size: u64,
    execution: &ExecutionControl,
) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut chunk = [0_u8; HASH_CHUNK_BYTES];
    loop {
        execution.checkpoint()?;
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .context("artifact size overflow while hashing")?;
        if size > expected_size {
            bail!("artifact changed while being hashed");
        }
        hasher.update(&chunk[..read]);
    }
    if size != expected_size {
        bail!("artifact changed while being hashed");
    }
    Ok(hex::encode(hasher.finalize()))
}
