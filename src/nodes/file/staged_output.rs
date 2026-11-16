//! Publish one node output file through the rooted, no-follow, atomic writer.
//!
//! Nodes that encode straight to a destination (images, generated PDFs, raw
//! provider bodies) call [`publish`] from their tracked blocking step instead
//! of creating the destination themselves.

use std::fs::File;
use std::io::{BufWriter, Error, Result as IoResult, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{Context as _, Result};

use crate::util::execution::ExecutionControl;

use super::RootedDir;

/// Largest slice handed to the staged file between two cancellation checks.
const CHUNK_BYTES: usize = 64 * 1024;

/// Buffered handle on the staged file that an encoder writes to.
pub(crate) type OutputWriter<'a> = BufWriter<StagedOutput<'a>>;

/// The staged file, observing cancellation and the step deadline per write.
pub(crate) struct StagedOutput<'a> {
    file: &'a mut File,
    execution: &'a ExecutionControl,
}

impl Write for StagedOutput<'_> {
    fn write(&mut self, buffer: &[u8]) -> IoResult<usize> {
        self.execution.checkpoint().map_err(Error::other)?;
        let chunk = &buffer[..buffer.len().min(CHUNK_BYTES)];
        self.file.write(chunk)
    }

    fn flush(&mut self) -> IoResult<()> {
        self.file.flush()
    }
}

impl Seek for StagedOutput<'_> {
    fn seek(&mut self, position: SeekFrom) -> IoResult<u64> {
        self.file.seek(position)
    }
}

/// Encode into a staged sibling of `destination`, then replace it atomically.
///
/// The destination directory must exist: these nodes never created parent
/// directories. A destination that is a symlink, a dangling link or a special
/// file is refused, and an existing regular file is replaced only after
/// `encode` succeeded and the staged bytes were synchronized. An encoder
/// error, a write error or cancellation removes the staged file and leaves
/// the destination as it was.
pub(crate) fn publish(
    destination: &Path,
    operation: &'static str,
    execution: &ExecutionControl,
    encode: impl FnOnce(&mut OutputWriter<'_>) -> Result<()>,
) -> Result<()> {
    execution.checkpoint()?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let leaf = destination
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{operation}: output path has no file name"))?;
    let root = RootedDir::existing(parent, operation, execution)?;
    let mut staged = root.stage_file(Path::new(leaf), true, execution)?;
    {
        let mut writer = BufWriter::new(StagedOutput {
            file: staged.writer(),
            execution,
        });
        let encoded = encode(&mut writer).and_then(|()| {
            writer
                .flush()
                .with_context(|| format!("{operation}: failed to write the output file"))
        });
        if let Err(error) = encoded {
            // A cancelled write surfaces as the encoder's own I/O error;
            // report the cancellation or the deadline as itself.
            execution.checkpoint()?;
            return Err(error);
        }
    }
    staged
        .writer()
        .sync_all()
        .with_context(|| format!("{operation}: failed to synchronize the output file"))?;
    execution.checkpoint()?;
    staged.commit()
}

#[cfg(test)]
mod tests;
