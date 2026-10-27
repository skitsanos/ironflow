use super::model::MAX_OUTPUT;
use crate::util::execution::ExecutionControl;
use std::io::{self, Seek, SeekFrom, Write};

pub(super) struct CappedWriter<'a> {
    pub file: &'a mut std::fs::File,
    pub execution: &'a ExecutionControl,
    pub position: u64,
}

impl Write for CappedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.execution.checkpoint().map_err(io::Error::other)?;
        if self.position.saturating_add(bytes.len() as u64) > MAX_OUTPUT as u64 {
            return Err(io::Error::other("document exceeds 32 MiB output limit"));
        }
        let written = self.file.write(bytes)?;
        self.position += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Seek for CappedWriter<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.execution.checkpoint().map_err(io::Error::other)?;
        self.position = self.file.seek(from)?;
        if self.position > MAX_OUTPUT as u64 {
            return Err(io::Error::other("document seek exceeds output limit"));
        }
        Ok(self.position)
    }
}
