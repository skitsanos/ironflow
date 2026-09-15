use std::io::{Error, ErrorKind, Result as IoResult, Write};
use std::path::Path;

use anyhow::Result;
use lopdf::Document;

use crate::nodes::file::RootedDir;
use crate::util::execution::ExecutionControl;

pub(super) struct Policy {
    pub operation: &'static str,
    pub variable: &'static str,
    pub maximum: u64,
    pub overwrite: bool,
}

pub(super) fn save_atomic(
    document: &mut Document,
    root: &RootedDir,
    leaf: &Path,
    policy: Policy,
    execution: &ExecutionControl,
) -> Result<()> {
    let mut staged = root.stage_file(leaf, policy.overwrite, execution)?;
    {
        let mut writer = CappedWriter {
            inner: staged.writer(),
            policy: &policy,
            written: 0,
            execution,
        };
        document.save_to(&mut writer).map_err(|error| {
            anyhow::anyhow!("{}: failed to save PDF: {error:?}", policy.operation)
        })?;
        writer.flush()?;
    }
    staged.writer().sync_all()?;
    execution.checkpoint()?;
    staged.commit()
}

struct CappedWriter<'a> {
    inner: &'a mut std::fs::File,
    policy: &'a Policy,
    written: u64,
    execution: &'a ExecutionControl,
}

impl Write for CappedWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> IoResult<usize> {
        self.execution.checkpoint().map_err(Error::other)?;
        let next = self.written.saturating_add(buffer.len() as u64);
        if next > self.policy.maximum {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "PDF exceeds {} ({})",
                    self.policy.variable, self.policy.maximum
                ),
            ));
        }
        let written = self.inner.write(buffer)?;
        self.written = self.written.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> IoResult<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests;
