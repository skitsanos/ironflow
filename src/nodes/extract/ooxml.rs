//! Guarded, cumulative reads for DOCX and PPTX ZIP packages.

use std::fs::File;
use std::io::BufRead;
use std::path::Path;

use anyhow::{Context, Result};
use zip::result::ZipError;

use super::resource::Limits;
use crate::artifacts::{ArtifactRef, LocalArtifactStore};
use crate::util::execution::ExecutionControl;

mod part_reader;

pub(super) struct Archive {
    inner: zip::ZipArchive<File>,
    operation: &'static str,
    actual_bytes: u64,
    max_actual_bytes: u64,
    max_part_bytes: u64,
}

impl Archive {
    pub(super) fn open(
        path: &Path,
        operation: &'static str,
        limits: Limits,
        execution: &ExecutionControl,
    ) -> Result<Self> {
        execution.checkpoint()?;
        let mut file = crate::util::bounded_read::open_regular_file(path, operation)?;
        super::xlsx::archive_preflight::check(
            &mut file,
            path,
            operation,
            limits.max_zip_entries,
            crate::util::limits::max_file_bytes(),
            "IRONFLOW_MAX_FILE_BYTES",
            Some(execution),
        )?;
        execution.checkpoint()?;
        let mut inner = zip::ZipArchive::new(file).map_err(|error| {
            anyhow::anyhow!(
                "{operation}: '{}' is not a valid OOXML archive: {error}",
                path.display()
            )
        })?;
        execution.checkpoint()?;
        preflight_entries(&mut inner, operation, limits, execution)?;
        Ok(Self {
            inner,
            operation,
            actual_bytes: 0,
            max_actual_bytes: limits.max_zip_bytes,
            max_part_bytes: limits.max_zip_bytes.min(limits.max_output_bytes),
        })
    }

    pub(super) fn with_required_xml<T>(
        &mut self,
        name: &str,
        execution: &ExecutionControl,
        parse: impl FnOnce(&mut dyn BufRead) -> Result<T>,
    ) -> Result<T> {
        self.with_optional_xml(name, execution, parse)?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{}: required archive part is missing: {name}",
                    self.operation
                )
            })
    }

    pub(super) fn with_optional_xml<T>(
        &mut self,
        name: &str,
        execution: &ExecutionControl,
        parse: impl FnOnce(&mut dyn BufRead) -> Result<T>,
    ) -> Result<Option<T>> {
        execution.checkpoint()?;
        let remaining = self.max_actual_bytes.saturating_sub(self.actual_bytes);
        let max_part_bytes = self.max_part_bytes.min(remaining);
        let entry = match self.inner.by_name(name) {
            Ok(entry) => entry,
            Err(ZipError::FileNotFound) => return Ok(None),
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "{}: cannot open archive part '{name}': {error}",
                    self.operation
                ));
            }
        };
        let mut reader =
            part_reader::PartReader::new(entry, max_part_bytes, name, self.operation, execution);
        let result = part_reader::parse_to_end(&mut reader, parse);
        let decoded_bytes = reader.bytes_read();
        drop(reader);
        self.actual_bytes = self.actual_bytes.saturating_add(decoded_bytes);
        execution.checkpoint()?;
        result.map(Some)
    }

    pub(super) fn store_optional_part(
        &mut self,
        name: &str,
        store: &LocalArtifactStore,
        mime_type: Option<String>,
        execution: &ExecutionControl,
    ) -> Result<Option<ArtifactRef>> {
        execution.checkpoint()?;
        let remaining = self.max_actual_bytes.saturating_sub(self.actual_bytes);
        let max_part_bytes = self.max_part_bytes.min(remaining);
        let artifact = {
            let entry = match self.inner.by_name(name) {
                Ok(entry) => entry,
                Err(ZipError::FileNotFound) => return Ok(None),
                Err(error) => {
                    return Err(anyhow::anyhow!(
                        "{}: cannot open archive part '{name}': {error}",
                        self.operation
                    ));
                }
            };
            store
                .put_reader(entry, max_part_bytes, mime_type, execution)
                .with_context(|| {
                    format!("{}: cannot store archive part '{name}'", self.operation)
                })?
        };
        self.actual_bytes = self.actual_bytes.saturating_add(artifact.size_bytes);
        execution.checkpoint()?;
        Ok(Some(artifact))
    }

    pub(super) fn entry_names(
        &mut self,
        prefix: &str,
        suffix: &str,
        execution: &ExecutionControl,
    ) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for index in 0..self.inner.len() {
            execution.checkpoint()?;
            let entry = self.inner.by_index(index).map_err(|error| {
                anyhow::anyhow!(
                    "{}: cannot inspect archive entry {index}: {error}",
                    self.operation
                )
            })?;
            let name = entry.name();
            if name.starts_with(prefix) && name.ends_with(suffix) {
                names.push(name.to_string());
            }
        }
        Ok(names)
    }
}

fn preflight_entries(
    archive: &mut zip::ZipArchive<File>,
    operation: &str,
    limits: Limits,
    execution: &ExecutionControl,
) -> Result<()> {
    let count = u64::try_from(archive.len()).unwrap_or(u64::MAX);
    if count > limits.max_zip_entries {
        anyhow::bail!(
            "{operation}: archive has {count} entries, exceeding IRONFLOW_MAX_ZIP_ENTRIES ({})",
            limits.max_zip_entries
        );
    }

    let mut declared_bytes = 0_u64;
    for index in 0..archive.len() {
        execution.checkpoint()?;
        let entry = archive.by_index(index)?;
        validate_entry_type(&entry, operation)?;
        declared_bytes = declared_bytes.saturating_add(entry.size());
        if declared_bytes > limits.max_zip_bytes {
            anyhow::bail!(
                "{operation}: declared uncompressed archive data exceeds the \
                 IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES limit ({})",
                limits.max_zip_bytes
            );
        }
    }
    execution.checkpoint()
}

fn validate_entry_type(entry: &zip::read::ZipFile<'_, File>, operation: &str) -> Result<()> {
    if entry.is_symlink() {
        anyhow::bail!(
            "{operation}: archive symlink parts are not supported: {}",
            entry.name()
        );
    }
    let expected = if entry.is_dir() { 0o040000 } else { 0o100000 };
    if let Some(mode) = entry.unix_mode() {
        let file_type = mode & 0o170000;
        if file_type != 0 && file_type != expected {
            anyhow::bail!(
                "{operation}: archive part is not a regular file or directory: {}",
                entry.name()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "ooxml/tests.rs"]
mod tests;
