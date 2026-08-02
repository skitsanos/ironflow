use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Result, bail};

use super::filesystem::{self, TempArtifact};
use super::{ArtifactRef, LocalArtifactStore};
use crate::util::execution::ExecutionControl;

const COPY_CHUNK_BYTES: usize = 16 * 1024;

/// A private temporary pathname backed by a verified artifact copy.
pub struct VerifiedPathLease {
    pub(super) temporary: TempArtifact,
}

impl VerifiedPathLease {
    /// Return the pathname accepted by a path-only third-party API.
    pub fn path(&self) -> &Path {
        self.temporary.path()
    }
}

impl LocalArtifactStore {
    /// Copy a verified descriptor into a private, read-only temporary path for
    /// a third-party API that cannot accept an open handle.
    ///
    /// The random pathname is removed on drop. The lease keeps its file handle
    /// open and never exposes the content-addressed store path. This prevents a
    /// path replacement from changing the leased inode, but another process
    /// with the same OS identity can still mutate it; the artifact directory
    /// therefore remains a trusted process boundary.
    pub fn verified_path_lease(
        &self,
        artifact: &ArtifactRef,
        max_bytes: u64,
        execution: &ExecutionControl,
    ) -> Result<VerifiedPathLease> {
        let mut source = self.open(artifact, execution)?;
        if artifact.size_bytes > max_bytes {
            bail!("artifact exceeds the {max_bytes} byte lease limit");
        }
        let mut temporary = self.create_temporary()?;
        copy_bounded(
            &mut source,
            temporary.file_mut(),
            artifact.size_bytes,
            execution,
        )?;
        temporary.file_mut().flush()?;
        temporary.file().sync_all()?;
        filesystem::harden_file(temporary.file())?;
        execution.checkpoint()?;
        Ok(VerifiedPathLease { temporary })
    }
}

fn copy_bounded(
    source: &mut File,
    destination: &mut File,
    expected_size: u64,
    execution: &ExecutionControl,
) -> Result<()> {
    let mut copied = 0_u64;
    let mut chunk = [0_u8; COPY_CHUNK_BYTES];
    loop {
        execution.checkpoint()?;
        let read = source.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        copied = copied.saturating_add(read as u64);
        if copied > expected_size {
            bail!("artifact changed while creating verified path lease");
        }
        destination.write_all(&chunk[..read])?;
    }
    if copied != expected_size {
        bail!("artifact changed while creating verified path lease");
    }
    Ok(())
}
