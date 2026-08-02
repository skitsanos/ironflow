use std::fs::File;
use std::io::{Error, ErrorKind, Read, Result as IoResult, Seek, SeekFrom};

use anyhow::Result;

use crate::util::execution::ExecutionControl;

pub(crate) struct CappedFile {
    file: File,
    limit: u64,
    label: String,
    limit_name: &'static str,
    execution: ExecutionControl,
}

impl CappedFile {
    pub(crate) fn from_file(
        file: File,
        label: String,
        limit: u64,
        limit_name: &'static str,
        execution: &ExecutionControl,
    ) -> Result<Self> {
        execution.checkpoint()?;
        let reader = Self {
            file,
            limit,
            label,
            limit_name,
            execution: execution.clone(),
        };
        reader.validate_length()?;
        Ok(reader)
    }

    fn validate_length(&self) -> IoResult<u64> {
        self.checkpoint()?;
        let length = self.file.metadata()?.len();
        if length > self.limit {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "{} '{}' is {length} bytes, exceeds {} ({})",
                    self.operation_label(),
                    self.label,
                    self.limit_name,
                    self.limit
                ),
            ));
        }
        Ok(length)
    }

    fn operation_label(&self) -> &str {
        if self.limit_name == "IRONFLOW_MAX_PDF_BYTES" {
            "PDF input"
        } else {
            "image input"
        }
    }

    fn checkpoint(&self) -> IoResult<()> {
        self.execution.checkpoint().map_err(Error::other)
    }

    fn seek_target(&mut self, position: SeekFrom) -> IoResult<u64> {
        let length = self.validate_length()?;
        let current = self.file.stream_position()?;
        let target = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(offset) => i128::from(current) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(length) + i128::from(offset),
        };
        if target < 0 || target > i128::from(self.limit) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "{} seek exceeds {} ({})",
                    self.operation_label(),
                    self.limit_name,
                    self.limit
                ),
            ));
        }
        Ok(target as u64)
    }
}

impl Read for CappedFile {
    fn read(&mut self, buffer: &mut [u8]) -> IoResult<usize> {
        self.validate_length()?;
        let position = self.file.stream_position()?;
        let remaining = self.limit.saturating_sub(position);
        if remaining == 0 {
            return Ok(0);
        }
        let request = buffer
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let read = self.file.read(&mut buffer[..request])?;
        self.checkpoint()?;
        Ok(read)
    }
}

impl Seek for CappedFile {
    fn seek(&mut self, position: SeekFrom) -> IoResult<u64> {
        let target = self.seek_target(position)?;
        let position = self.file.seek(SeekFrom::Start(target))?;
        self.checkpoint()?;
        Ok(position)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[tokio::test]
    async fn detects_growth_after_open_and_rejects_out_of_range_seeks() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.bin");
        std::fs::write(&path, b"1234").unwrap();
        crate::util::execution::run_tracked_blocking_step(move |execution| {
            let file = crate::util::bounded_read::open_regular_file(&path, "test")?;
            let mut reader = CappedFile::from_file(
                file,
                path.display().to_string(),
                4,
                "IRONFLOW_MAX_IMAGE_ENCODED_BYTES",
                &execution,
            )?;
            assert!(reader.seek(SeekFrom::Start(5)).is_err());
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)?
                .write_all(b"5")?;
            assert!(reader.read(&mut [0; 1]).is_err());
            Ok(())
        })
        .await
        .unwrap();
    }
}
