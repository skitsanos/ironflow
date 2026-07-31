//! Capped, cancellation-aware loading for `lopdf` consumers.

use std::fs::File;
use std::io::{Error, ErrorKind, Read, Result as IoResult};
use std::path::Path;

use anyhow::Result;
use lopdf::Document;

use crate::util::execution::ExecutionControl;

pub(super) fn load_document(
    path: &str,
    operation: &str,
    execution: &ExecutionControl,
) -> Result<Document> {
    execution.checkpoint()?;
    let maximum = crate::util::limits::max_pdf_bytes();
    let file = crate::util::bounded_read::open_regular_file(Path::new(path), operation)?;
    let declared = file.metadata()?.len();
    if declared > maximum {
        anyhow::bail!(
            "{operation}: PDF '{path}' is {declared} bytes, exceeds IRONFLOW_MAX_PDF_BYTES ({maximum})"
        );
    }
    let reader = CappedReader::new(file, maximum, operation, execution);
    let document = Document::load_from(reader)
        .map_err(|error| anyhow::anyhow!("{operation}: failed to load '{path}': {error:?}"))?;
    execution.checkpoint()?;
    Ok(document)
}

struct CappedReader<'a> {
    file: File,
    maximum: u64,
    read: u64,
    operation: &'a str,
    execution: &'a ExecutionControl,
}

impl<'a> CappedReader<'a> {
    fn new(file: File, maximum: u64, operation: &'a str, execution: &'a ExecutionControl) -> Self {
        Self {
            file,
            maximum,
            read: 0,
            operation,
            execution,
        }
    }

    fn checkpoint(&self) -> IoResult<()> {
        self.execution.checkpoint().map_err(Error::other)
    }

    fn limit_error(&self) -> Error {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "{}: PDF input exceeds IRONFLOW_MAX_PDF_BYTES ({})",
                self.operation, self.maximum
            ),
        )
    }
}

impl Read for CappedReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> IoResult<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        self.checkpoint()?;
        if self.read == self.maximum {
            let mut probe = [0_u8; 1];
            return match self.file.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(self.limit_error()),
            };
        }
        let remaining = self.maximum.saturating_sub(self.read);
        let request = buffer
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let read = self.file.read(&mut buffer[..request])?;
        self.read = self.read.saturating_add(read as u64);
        self.checkpoint()?;
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[tokio::test]
    async fn capped_reader_detects_growth_after_open() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("document.pdf");
        std::fs::write(&path, b"1234").unwrap();

        crate::util::execution::run_tracked_blocking_step(move |execution| {
            let file = crate::util::bounded_read::open_regular_file(&path, "test")?;
            let mut reader = CappedReader::new(file, 4, "test", &execution);
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)?
                .write_all(b"5")?;
            let error = std::io::read_to_string(&mut reader).unwrap_err();
            assert!(error.to_string().contains("IRONFLOW_MAX_PDF_BYTES"));
            Ok(())
        })
        .await
        .unwrap();
    }
}
