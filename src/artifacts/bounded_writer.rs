use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};

use anyhow::Result;

use crate::util::execution::ExecutionControl;

pub(crate) struct BoundedArtifactWriter<'a> {
    inner: &'a mut File,
    execution: &'a ExecutionControl,
    max_bytes: u64,
    position: u64,
    length: u64,
}

impl<'a> BoundedArtifactWriter<'a> {
    pub(crate) fn new(
        inner: &'a mut File,
        max_bytes: u64,
        execution: &'a ExecutionControl,
    ) -> Result<Self> {
        execution.checkpoint()?;
        let position = inner.stream_position()?;
        let length = inner.metadata()?.len();
        if position > max_bytes || length > max_bytes {
            anyhow::bail!("artifact exceeds the {max_bytes} byte limit");
        }
        Ok(Self {
            inner,
            execution,
            max_bytes,
            position,
            length,
        })
    }

    pub(crate) fn len(&self) -> u64 {
        self.length
    }

    fn checkpoint(&self) -> io::Result<()> {
        self.execution.checkpoint().map_err(io::Error::other)
    }

    fn check_position(&self, position: u64) -> io::Result<()> {
        if position > self.max_bytes {
            return Err(io::Error::other(format!(
                "artifact exceeds the {} byte limit",
                self.max_bytes
            )));
        }
        Ok(())
    }
}

impl Write for BoundedArtifactWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.checkpoint()?;
        let end = self
            .position
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| io::Error::other("artifact size overflow"))?;
        self.check_position(end)?;
        let written = self.inner.write(buffer)?;
        self.position = self.position.saturating_add(written as u64);
        self.length = self.length.max(self.position);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.checkpoint()?;
        self.inner.flush()
    }
}

impl Seek for BoundedArtifactWriter<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.checkpoint()?;
        let target = match position {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(offset) => apply_offset(self.position, offset),
            SeekFrom::End(offset) => apply_offset(self.length, offset),
        }
        .ok_or_else(|| io::Error::other("artifact seek is outside the file range"))?;
        self.check_position(target)?;
        let actual = self.inner.seek(SeekFrom::Start(target))?;
        self.position = actual;
        Ok(actual)
    }
}

fn apply_offset(base: u64, offset: i64) -> Option<u64> {
    if offset >= 0 {
        base.checked_add(offset as u64)
    } else {
        base.checked_sub(offset.unsigned_abs())
    }
}
