//! Cooperative, UTF-8-validating reads for one OOXML ZIP member.

use std::io::{self, BufRead, BufReader, Read};

use anyhow::Result;

use crate::util::execution::ExecutionControl;

const BUFFER_BYTES: usize = 16 * 1024;

pub(super) struct PartReader<'a, R> {
    inner: R,
    max_bytes: u64,
    bytes_read: u64,
    utf8_tail: Vec<u8>,
    validation: Vec<u8>,
    name: &'a str,
    operation: &'a str,
    execution: &'a ExecutionControl,
}

impl<'a, R> PartReader<'a, R> {
    pub(super) fn new(
        inner: R,
        max_bytes: u64,
        name: &'a str,
        operation: &'a str,
        execution: &'a ExecutionControl,
    ) -> Self {
        Self {
            inner,
            max_bytes,
            bytes_read: 0,
            utf8_tail: Vec::with_capacity(3),
            validation: Vec::with_capacity(BUFFER_BYTES + 3),
            name,
            operation,
            execution,
        }
    }

    pub(super) fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    fn checkpoint(&self) -> io::Result<()> {
        self.execution.checkpoint().map_err(io::Error::other)
    }

    fn limit_error(&self) -> io::Error {
        io::Error::other(format!(
            "{}: decoded archive part '{}' exceeds the cumulative or per-part extraction limit \
             ({} bytes)",
            self.operation, self.name, self.max_bytes
        ))
    }

    fn validate_utf8(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.validation.clear();
        self.validation.extend_from_slice(&self.utf8_tail);
        self.validation.extend_from_slice(bytes);
        self.utf8_tail.clear();
        match std::str::from_utf8(&self.validation) {
            Ok(_) => Ok(()),
            Err(error) if error.error_len().is_none() => {
                self.utf8_tail
                    .extend_from_slice(&self.validation[error.valid_up_to()..]);
                Ok(())
            }
            Err(_) => Err(self.utf8_error()),
        }
    }

    fn validate_eof(&self) -> io::Result<()> {
        if self.utf8_tail.is_empty() {
            Ok(())
        } else {
            Err(self.utf8_error())
        }
    }

    fn utf8_error(&self) -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{}: archive part is not UTF-8: {}",
                self.operation, self.name
            ),
        )
    }
}

impl<R: Read> Read for PartReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        self.checkpoint()?;
        let remaining = self.max_bytes.saturating_sub(self.bytes_read);
        if remaining == 0 {
            let mut probe = [0_u8; 1];
            let read = self.read_inner(&mut probe)?;
            if read == 0 {
                self.validate_eof()?;
                return Ok(0);
            }
            self.bytes_read = self.bytes_read.saturating_add(read as u64);
            return Err(self.limit_error());
        }

        let request = buffer
            .len()
            .min(BUFFER_BYTES)
            .min(remaining.try_into().unwrap_or(usize::MAX));
        let read = self.read_inner(&mut buffer[..request])?;
        if read == 0 {
            self.validate_eof()?;
            return Ok(0);
        }
        self.bytes_read = self.bytes_read.saturating_add(read as u64);
        self.validate_utf8(&buffer[..read])?;
        Ok(read)
    }
}

impl<R: Read> PartReader<'_, R> {
    fn read_inner(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buffer).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "{}: cannot decode archive part '{}': {error}",
                    self.operation, self.name
                ),
            )
        })
    }
}

pub(super) fn parse_to_end<R: Read, T>(
    reader: &mut PartReader<'_, R>,
    parse: impl FnOnce(&mut dyn BufRead) -> Result<T>,
) -> Result<T> {
    let mut buffered = BufReader::with_capacity(BUFFER_BYTES, reader);
    let value = parse(&mut buffered)?;
    // A parser callback may deliberately stop after the data it needs. Drain
    // successful callbacks so the ZIP reader reaches EOF and verifies CRC.
    let mut chunk = [0_u8; BUFFER_BYTES];
    loop {
        let read = buffered.read(&mut chunk)?;
        if read == 0 {
            break;
        }
    }
    Ok(value)
}
