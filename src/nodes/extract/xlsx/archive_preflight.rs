//! Bounded ZIP end-record validation before `zip` allocates entry metadata.

mod central_directory;
mod fields;

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::Result;

use crate::util::execution::ExecutionControl;

use fields::{le_u16, le_u32, le_u64};

const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
const ZIP64_EOCD_SIGNATURE: &[u8; 4] = b"PK\x06\x06";
const ZIP64_LOCATOR_SIGNATURE: &[u8; 4] = b"PK\x06\x07";
const EOCD_BYTES: usize = 22;
const MAX_COMMENT_BYTES: usize = u16::MAX as usize;
const ZIP64_LOCATOR_BYTES: u64 = 20;
const ZIP64_EOCD_MIN_BYTES: u64 = 56;

#[derive(Clone, Copy, Debug)]
pub(super) struct Directory {
    pub(super) entries: u64,
    pub(super) offset: u64,
    pub(super) size: u64,
    pub(super) end: u64,
}

#[derive(Clone, Copy, Debug)]
struct ClassicDirectory {
    disk: u16,
    directory_disk: u16,
    entries_on_disk: u16,
    entries: u16,
    size: u32,
    offset: u32,
}

/// Validate raw archive metadata before `ZipArchive::new` can reserve one
/// `ZipFileData` per caller-controlled EOCD entry count.
pub(in crate::nodes::extract) fn check<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    operation: &str,
    max_entries: u64,
    max_raw_bytes: u64,
    raw_limit_name: &str,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    check_with_limits(
        reader,
        path,
        operation,
        central_directory::Limits {
            max_entries,
            max_raw_bytes,
            raw_limit_name,
            max_metadata_bytes: max_raw_bytes,
            metadata_limit_name: raw_limit_name,
        },
        execution,
    )
}

/// XLSX-specific raw preflight with an independent metadata ceiling.
pub(super) fn check_xlsx<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    max_entries: u64,
    max_raw_bytes: u64,
    max_metadata_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    check_with_limits(
        reader,
        path,
        "extract_xlsx",
        central_directory::Limits {
            max_entries,
            max_raw_bytes,
            raw_limit_name: "IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES",
            max_metadata_bytes,
            metadata_limit_name: "IRONFLOW_MAX_XLSX_ARCHIVE_METADATA_BYTES",
        },
        execution,
    )
}

fn check_with_limits<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    operation: &str,
    limits: central_directory::Limits<'_>,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    checkpoint(execution)?;
    let file_len = reader.seek(SeekFrom::End(0))?;
    if file_len > limits.max_raw_bytes {
        anyhow::bail!(
            "{operation}: '{}' is {file_len} bytes, exceeding {} ({})",
            path.display(),
            limits.raw_limit_name,
            limits.max_raw_bytes
        );
    }
    if file_len < EOCD_BYTES as u64 {
        anyhow::bail!(
            "{operation}: '{}' is missing a ZIP end record",
            path.display()
        );
    }

    let tail_len = file_len.min((EOCD_BYTES + MAX_COMMENT_BYTES) as u64) as usize;
    let tail_start = file_len - tail_len as u64;
    reader.seek(SeekFrom::Start(tail_start))?;
    let mut tail = vec![0_u8; tail_len];
    reader.read_exact(&mut tail)?;
    checkpoint(execution)?;

    let mut structural_error = None;
    for index in (0..=tail_len - EOCD_BYTES).rev() {
        if &tail[index..index + 4] != EOCD_SIGNATURE {
            continue;
        }
        let comment_len = le_u16(&tail[index + 20..index + 22]) as usize;
        if index + EOCD_BYTES + comment_len != tail_len {
            continue;
        }

        let eocd_offset = tail_start + index as u64;
        match parse_directory(
            reader,
            &tail[index..index + EOCD_BYTES],
            eocd_offset,
            operation,
        ) {
            Ok(directory) => {
                central_directory::validate(reader, path, operation, directory, limits, execution)?;
                checkpoint(execution)?;
                return Ok(());
            }
            Err(error) => structural_error = Some(error),
        }
    }

    Err(structural_error.unwrap_or_else(|| {
        anyhow::anyhow!(
            "{operation}: '{}' is missing a valid ZIP end record",
            path.display()
        )
    }))
}

fn parse_directory<R: Read + Seek>(
    reader: &mut R,
    eocd: &[u8],
    eocd_offset: u64,
    operation: &str,
) -> Result<Directory> {
    let classic = ClassicDirectory {
        disk: le_u16(&eocd[4..6]),
        directory_disk: le_u16(&eocd[6..8]),
        entries_on_disk: le_u16(&eocd[8..10]),
        entries: le_u16(&eocd[10..12]),
        size: le_u32(&eocd[12..16]),
        offset: le_u32(&eocd[16..20]),
    };
    let uses_zip64 = classic.disk == u16::MAX
        || classic.directory_disk == u16::MAX
        || classic.entries_on_disk == u16::MAX
        || classic.entries == u16::MAX
        || classic.size == u32::MAX
        || classic.offset == u32::MAX;

    if !uses_zip64 {
        if classic.disk != 0
            || classic.directory_disk != 0
            || classic.entries_on_disk != classic.entries
        {
            anyhow::bail!("{operation}: multi-disk ZIP archives are not supported");
        }
        return Ok(Directory {
            entries: u64::from(classic.entries),
            offset: u64::from(classic.offset),
            size: u64::from(classic.size),
            end: eocd_offset,
        });
    }

    parse_zip64_directory(reader, eocd_offset, classic, operation)
}

fn parse_zip64_directory<R: Read + Seek>(
    reader: &mut R,
    eocd_offset: u64,
    classic: ClassicDirectory,
    operation: &str,
) -> Result<Directory> {
    let locator_offset = eocd_offset
        .checked_sub(ZIP64_LOCATOR_BYTES)
        .ok_or_else(|| anyhow::anyhow!("{operation}: ZIP64 locator is missing"))?;
    let mut locator = [0_u8; ZIP64_LOCATOR_BYTES as usize];
    read_exact_at(reader, locator_offset, &mut locator)?;
    if &locator[..4] != ZIP64_LOCATOR_SIGNATURE {
        anyhow::bail!("{operation}: ZIP64 locator is missing or truncated");
    }
    if le_u32(&locator[4..8]) != 0 || le_u32(&locator[16..20]) != 1 {
        anyhow::bail!("{operation}: multi-disk ZIP64 archives are not supported");
    }

    let record_offset = le_u64(&locator[8..16]);
    if record_offset >= locator_offset
        || locator_offset.saturating_sub(record_offset) < ZIP64_EOCD_MIN_BYTES
    {
        anyhow::bail!("{operation}: ZIP64 end record has inconsistent bounds");
    }
    let mut record = [0_u8; ZIP64_EOCD_MIN_BYTES as usize];
    read_exact_at(reader, record_offset, &mut record)?;
    if &record[..4] != ZIP64_EOCD_SIGNATURE {
        anyhow::bail!("{operation}: ZIP64 end record is missing or truncated");
    }
    let record_size = le_u64(&record[4..12]);
    if record_size < 44
        || record_offset
            .checked_add(12)
            .and_then(|position| position.checked_add(record_size))
            != Some(locator_offset)
    {
        anyhow::bail!("{operation}: ZIP64 end record has inconsistent bounds");
    }

    let disk = le_u32(&record[16..20]);
    let directory_disk = le_u32(&record[20..24]);
    let entries_on_disk = le_u64(&record[24..32]);
    let entries = le_u64(&record[32..40]);
    let size = le_u64(&record[40..48]);
    let offset = le_u64(&record[48..56]);
    if disk != 0 || directory_disk != 0 || entries_on_disk != entries {
        anyhow::bail!("{operation}: multi-disk ZIP64 archives are not supported");
    }
    if (classic.disk != u16::MAX && u32::from(classic.disk) != disk)
        || (classic.directory_disk != u16::MAX
            && u32::from(classic.directory_disk) != directory_disk)
        || (classic.entries_on_disk != u16::MAX
            && u64::from(classic.entries_on_disk) != entries_on_disk)
    {
        anyhow::bail!("{operation}: ZIP and ZIP64 disk metadata disagree");
    }
    if classic.entries != u16::MAX && u64::from(classic.entries) != entries {
        anyhow::bail!("{operation}: ZIP and ZIP64 entry counts disagree");
    }
    if classic.size != u32::MAX && u64::from(classic.size) != size {
        anyhow::bail!("{operation}: ZIP and ZIP64 directory sizes disagree");
    }
    if classic.offset != u32::MAX && u64::from(classic.offset) != offset {
        anyhow::bail!("{operation}: ZIP and ZIP64 directory offsets disagree");
    }

    Ok(Directory {
        entries,
        offset,
        size,
        end: record_offset,
    })
}

fn read_exact_at<R: Read + Seek>(reader: &mut R, offset: u64, buffer: &mut [u8]) -> Result<()> {
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(buffer)?;
    Ok(())
}

fn checkpoint(execution: Option<&ExecutionControl>) -> Result<()> {
    if let Some(execution) = execution {
        execution.checkpoint()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "archive_preflight/tests.rs"]
mod tests;
