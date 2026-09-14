//! OOXML-specific limits for the shared raw ZIP preflight.

use std::io::{Read, Seek};
use std::path::Path;

use anyhow::Result;

use crate::util::execution::ExecutionControl;
use crate::util::zip_preflight::{self, Limits};

pub(in crate::nodes::extract) fn check<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    operation: &str,
    max_entries: u64,
    max_raw_bytes: u64,
    raw_limit_name: &str,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    zip_preflight::check(
        reader,
        path,
        operation,
        Limits {
            max_entries,
            max_raw_bytes,
            raw_limit_name,
            max_metadata_bytes: max_raw_bytes,
            metadata_limit_name: raw_limit_name,
        },
        execution,
    )
    .map(|_| ())
}

pub(super) fn check_xlsx<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    max_entries: u64,
    max_raw_bytes: u64,
    max_metadata_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    zip_preflight::check(
        reader,
        path,
        "extract_xlsx",
        Limits {
            max_entries,
            max_raw_bytes,
            raw_limit_name: "IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES",
            max_metadata_bytes,
            metadata_limit_name: "IRONFLOW_MAX_XLSX_ARCHIVE_METADATA_BYTES",
        },
        execution,
    )
    .map(|_| ())
}

#[cfg(test)]
#[path = "archive_preflight/tests.rs"]
mod tests;
