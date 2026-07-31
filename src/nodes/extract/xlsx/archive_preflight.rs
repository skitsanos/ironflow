//! Bounded ZIP end-record validation before `zip` allocates entry metadata.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::Result;

use crate::util::execution::ExecutionControl;

const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
const ZIP64_EOCD_SIGNATURE: &[u8; 4] = b"PK\x06\x06";
const ZIP64_LOCATOR_SIGNATURE: &[u8; 4] = b"PK\x06\x07";
const CENTRAL_HEADER_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const EOCD_BYTES: usize = 22;
const MAX_COMMENT_BYTES: usize = u16::MAX as usize;
const ZIP64_LOCATOR_BYTES: u64 = 20;
const ZIP64_EOCD_MIN_BYTES: u64 = 56;
const CENTRAL_HEADER_MIN_BYTES: u64 = 46;

#[derive(Clone, Copy, Debug)]
struct Directory {
    entries: u64,
    offset: u64,
    size: u64,
    end: u64,
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
pub(super) fn check<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    max_entries: u64,
    max_archive_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    checkpoint(execution)?;
    let file_len = reader.seek(SeekFrom::End(0))?;
    if file_len > max_archive_bytes {
        anyhow::bail!(
            "extract_xlsx: '{}' is {file_len} bytes, exceeding the raw workbook bound from \
             IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES ({max_archive_bytes})",
            path.display()
        );
    }
    if file_len < EOCD_BYTES as u64 {
        anyhow::bail!(
            "extract_xlsx: '{}' is missing a ZIP end record",
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
        match parse_directory(reader, &tail[index..index + EOCD_BYTES], eocd_offset) {
            Ok(directory) => {
                validate_directory(
                    reader,
                    path,
                    directory,
                    file_len,
                    max_entries,
                    max_archive_bytes,
                )?;
                checkpoint(execution)?;
                return Ok(());
            }
            Err(error) => structural_error = Some(error),
        }
    }

    Err(structural_error.unwrap_or_else(|| {
        anyhow::anyhow!(
            "extract_xlsx: '{}' is missing a valid ZIP end record",
            path.display()
        )
    }))
}

fn parse_directory<R: Read + Seek>(
    reader: &mut R,
    eocd: &[u8],
    eocd_offset: u64,
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
            anyhow::bail!("extract_xlsx: multi-disk ZIP workbooks are not supported");
        }
        return Ok(Directory {
            entries: u64::from(classic.entries),
            offset: u64::from(classic.offset),
            size: u64::from(classic.size),
            end: eocd_offset,
        });
    }

    parse_zip64_directory(reader, eocd_offset, classic)
}

fn parse_zip64_directory<R: Read + Seek>(
    reader: &mut R,
    eocd_offset: u64,
    classic: ClassicDirectory,
) -> Result<Directory> {
    let locator_offset = eocd_offset
        .checked_sub(ZIP64_LOCATOR_BYTES)
        .ok_or_else(|| anyhow::anyhow!("extract_xlsx: ZIP64 locator is missing"))?;
    let mut locator = [0_u8; ZIP64_LOCATOR_BYTES as usize];
    read_exact_at(reader, locator_offset, &mut locator)?;
    if &locator[..4] != ZIP64_LOCATOR_SIGNATURE {
        anyhow::bail!("extract_xlsx: ZIP64 locator is missing or truncated");
    }
    if le_u32(&locator[4..8]) != 0 || le_u32(&locator[16..20]) != 1 {
        anyhow::bail!("extract_xlsx: multi-disk ZIP64 workbooks are not supported");
    }

    let record_offset = le_u64(&locator[8..16]);
    if record_offset >= locator_offset
        || locator_offset.saturating_sub(record_offset) < ZIP64_EOCD_MIN_BYTES
    {
        anyhow::bail!("extract_xlsx: ZIP64 end record has inconsistent bounds");
    }
    let mut record = [0_u8; ZIP64_EOCD_MIN_BYTES as usize];
    read_exact_at(reader, record_offset, &mut record)?;
    if &record[..4] != ZIP64_EOCD_SIGNATURE {
        anyhow::bail!("extract_xlsx: ZIP64 end record is missing or truncated");
    }
    let record_size = le_u64(&record[4..12]);
    if record_size < 44
        || record_offset
            .checked_add(12)
            .and_then(|position| position.checked_add(record_size))
            != Some(locator_offset)
    {
        anyhow::bail!("extract_xlsx: ZIP64 end record has inconsistent bounds");
    }

    let disk = le_u32(&record[16..20]);
    let directory_disk = le_u32(&record[20..24]);
    let entries_on_disk = le_u64(&record[24..32]);
    let entries = le_u64(&record[32..40]);
    let size = le_u64(&record[40..48]);
    let offset = le_u64(&record[48..56]);
    if disk != 0 || directory_disk != 0 || entries_on_disk != entries {
        anyhow::bail!("extract_xlsx: multi-disk ZIP64 workbooks are not supported");
    }
    if (classic.disk != u16::MAX && u32::from(classic.disk) != disk)
        || (classic.directory_disk != u16::MAX
            && u32::from(classic.directory_disk) != directory_disk)
        || (classic.entries_on_disk != u16::MAX
            && u64::from(classic.entries_on_disk) != entries_on_disk)
    {
        anyhow::bail!("extract_xlsx: ZIP and ZIP64 disk metadata disagree");
    }
    if classic.entries != u16::MAX && u64::from(classic.entries) != entries {
        anyhow::bail!("extract_xlsx: ZIP and ZIP64 entry counts disagree");
    }
    if classic.size != u32::MAX && u64::from(classic.size) != size {
        anyhow::bail!("extract_xlsx: ZIP and ZIP64 directory sizes disagree");
    }
    if classic.offset != u32::MAX && u64::from(classic.offset) != offset {
        anyhow::bail!("extract_xlsx: ZIP and ZIP64 directory offsets disagree");
    }

    Ok(Directory {
        entries,
        offset,
        size,
        end: record_offset,
    })
}

fn validate_directory<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    directory: Directory,
    file_len: u64,
    max_entries: u64,
    max_archive_bytes: u64,
) -> Result<()> {
    if directory.entries > max_entries {
        anyhow::bail!(
            "extract_xlsx: '{}' declares {} zip entries, exceeding \
             IRONFLOW_MAX_ZIP_ENTRIES ({max_entries})",
            path.display(),
            directory.entries
        );
    }
    usize::try_from(directory.entries)
        .map_err(|_| anyhow::anyhow!("extract_xlsx: ZIP entry count exceeds this platform"))?;
    if directory.size > max_archive_bytes {
        anyhow::bail!(
            "extract_xlsx: '{}' central directory is {} bytes, exceeding \
             IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES ({max_archive_bytes})",
            path.display(),
            directory.size
        );
    }
    let minimum = directory.entries.saturating_mul(CENTRAL_HEADER_MIN_BYTES);
    if directory.size < minimum {
        anyhow::bail!("extract_xlsx: ZIP central directory is smaller than its entry count");
    }
    let directory_end = directory
        .offset
        .checked_add(directory.size)
        .ok_or_else(|| anyhow::anyhow!("extract_xlsx: ZIP central-directory bounds overflow"))?;
    if directory_end > directory.end || directory_end > file_len {
        anyhow::bail!("extract_xlsx: ZIP central directory extends outside the workbook");
    }
    if directory.entries > 0 {
        let mut signature = [0_u8; 4];
        read_exact_at(reader, directory.offset, &mut signature)?;
        if &signature != CENTRAL_HEADER_SIGNATURE {
            anyhow::bail!("extract_xlsx: ZIP central directory has no entry header");
        }
    }
    Ok(())
}

fn read_exact_at<R: Read + Seek>(reader: &mut R, offset: u64, buffer: &mut [u8]) -> Result<()> {
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(buffer)?;
    Ok(())
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().expect("two-byte ZIP field"))
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("four-byte ZIP field"))
}

fn le_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("eight-byte ZIP field"))
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
