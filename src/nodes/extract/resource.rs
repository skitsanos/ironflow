use std::io::{self, Read, Write};
use std::path::Path;

use anyhow::{Context, Result};

use crate::engine::types::NodeOutput;
use crate::util::execution::ExecutionControl;

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub(super) max_output_bytes: u64,
    pub(super) max_items: u64,
    pub(super) max_zip_entries: u64,
    pub(super) max_zip_bytes: u64,
    pub(super) max_pdf_pages: u64,
}

impl Limits {
    pub(super) fn current() -> Self {
        Self {
            max_output_bytes: crate::util::limits::max_extract_output_bytes(),
            max_items: crate::util::limits::max_extract_items(),
            max_zip_entries: crate::util::limits::max_zip_entries(),
            max_zip_bytes: crate::util::limits::max_zip_uncompressed_bytes(),
            max_pdf_pages: crate::util::limits::max_pdf_extract_pages(),
        }
    }
}

pub(super) fn read_file(
    path: &Path,
    max_bytes: u64,
    operation: &str,
    execution: &ExecutionControl,
) -> Result<Vec<u8>> {
    execution.checkpoint()?;
    let mut file = crate::util::bounded_read::open_regular_file(path, operation)?;
    let declared = file.metadata()?.len();
    if declared > max_bytes {
        anyhow::bail!(
            "{operation}: '{}' is {declared} bytes, exceeds the {max_bytes} byte limit",
            path.display()
        );
    }

    let mut bytes = Vec::new();
    let read_limit = max_bytes.saturating_add(1);
    let mut chunk = [0_u8; 16 * 1024];
    let mut announced_read = false;
    while (bytes.len() as u64) < read_limit {
        execution.checkpoint()?;
        let remaining = read_limit.saturating_sub(bytes.len() as u64);
        let request = chunk.len().min(remaining.try_into().unwrap_or(usize::MAX));
        let read = file
            .read(&mut chunk[..request])
            .with_context(|| format!("{operation}: failed to read '{}'", path.display()))?;
        if read == 0 {
            break;
        }
        if !announced_read {
            tracing::trace!(
                target: "ironflow::extract::input",
                operation,
                "extractor input read started"
            );
            announced_read = true;
        }
        bytes.try_reserve_exact(read).with_context(|| {
            format!(
                "{operation}: cannot reserve memory for '{}'",
                path.display()
            )
        })?;
        bytes.extend_from_slice(&chunk[..read]);
    }
    if bytes.len() as u64 > max_bytes {
        anyhow::bail!(
            "{operation}: '{}' exceeds the {max_bytes} byte limit",
            path.display()
        );
    }
    execution.checkpoint()?;
    Ok(bytes)
}

pub(super) fn read_string(
    path: &Path,
    max_bytes: u64,
    operation: &str,
    execution: &ExecutionControl,
) -> Result<String> {
    String::from_utf8(read_file(path, max_bytes, operation, execution)?)
        .with_context(|| format!("{operation}: '{}' is not valid UTF-8", path.display()))
}

pub(super) struct Budget<'a> {
    operation: &'static str,
    execution: &'a ExecutionControl,
    max_items: u64,
    used_items: u64,
    max_output_bytes: u64,
    projected_output_bytes: u64,
}

impl<'a> Budget<'a> {
    pub(super) fn new(
        operation: &'static str,
        limits: Limits,
        execution: &'a ExecutionControl,
    ) -> Self {
        Self {
            operation,
            execution,
            max_items: limits.max_items,
            used_items: 0,
            max_output_bytes: limits.max_output_bytes,
            projected_output_bytes: 0,
        }
    }

    pub(super) fn checkpoint(&self) -> Result<()> {
        self.execution.checkpoint()
    }

    pub(super) fn charge_item(&mut self, what: &str) -> Result<()> {
        self.charge_items(1, what)
    }

    pub(super) fn charge_items(&mut self, count: u64, what: &str) -> Result<()> {
        self.checkpoint()?;
        self.used_items = self.used_items.saturating_add(count);
        if self.used_items > self.max_items {
            anyhow::bail!(
                "{}: {} exceeds IRONFLOW_MAX_EXTRACT_ITEMS ({})",
                self.operation,
                what,
                self.max_items
            );
        }
        Ok(())
    }

    pub(super) fn charge_output(&mut self, bytes: u64, what: &str) -> Result<()> {
        self.checkpoint()?;
        self.projected_output_bytes = self.projected_output_bytes.saturating_add(bytes);
        if self.projected_output_bytes > self.max_output_bytes {
            anyhow::bail!(
                "{}: {} exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES ({})",
                self.operation,
                what,
                self.max_output_bytes
            );
        }
        Ok(())
    }

    pub(super) fn inspect_html(&mut self, html: &str) -> Result<()> {
        for chunk in html.as_bytes().chunks(8 * 1024) {
            self.checkpoint()?;
            let tags = chunk.iter().filter(|byte| **byte == b'<').count() as u64;
            self.charge_items(tags, "HTML markup items")?;
        }
        Ok(())
    }

    pub(super) fn ensure_output(&self, output: &NodeOutput) -> Result<()> {
        self.checkpoint()?;
        let mut writer = CountingWriter::new(self.max_output_bytes);
        match serde_json::to_writer(&mut writer, output) {
            Ok(()) => Ok(()),
            Err(_) if writer.exceeded => anyhow::bail!(
                "{}: serialized result exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES ({})",
                self.operation,
                self.max_output_bytes
            ),
            Err(error) => Err(error.into()),
        }
    }
}

struct CountingWriter {
    count: u64,
    limit: u64,
    exceeded: bool,
}

impl CountingWriter {
    fn new(limit: u64) -> Self {
        Self {
            count: 0,
            limit,
            exceeded: false,
        }
    }
}

impl Write for CountingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.count = self.count.saturating_add(buffer.len() as u64);
        if self.count > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("extraction output limit exceeded"));
        }
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn output_limit_is_checked_without_materializing_an_extra_buffer() {
        crate::util::execution::run_blocking_step(|execution| {
            let limits = Limits {
                max_output_bytes: 8,
                max_items: 10,
                max_zip_entries: 10,
                max_zip_bytes: 10,
                max_pdf_pages: 10,
            };
            let budget = Budget::new("test", limits, &execution);
            let output = NodeOutput::from([(
                "value".to_string(),
                serde_json::Value::String("too large".to_string()),
            )]);
            let error = budget.ensure_output(&output).unwrap_err().to_string();
            assert!(
                error.contains("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
                "{error}"
            );
            Ok(())
        })
        .await
        .unwrap();
    }
}
