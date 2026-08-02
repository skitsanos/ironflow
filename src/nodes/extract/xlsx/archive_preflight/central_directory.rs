//! Exact, allocation-free traversal of ZIP central-directory headers.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest as _, Sha256};

use super::Directory;
use super::fields::le_u16;
use crate::util::execution::ExecutionControl;

const HEADER_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const HEADER_BYTES: u64 = 46;
const NAME_CHUNK_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy)]
struct NameLocation {
    offset: u64,
    length: u16,
}

#[derive(Clone, Copy)]
pub(super) struct Limits<'a> {
    pub(super) max_entries: u64,
    pub(super) max_raw_bytes: u64,
    pub(super) raw_limit_name: &'a str,
    pub(super) max_metadata_bytes: u64,
    pub(super) metadata_limit_name: &'a str,
}

pub(super) fn validate<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    operation: &str,
    directory: Directory,
    limits: Limits<'_>,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    validate_bounds(path, operation, directory, limits)?;
    let directory_end = directory.offset + directory.size;
    let mut cursor = directory.offset;
    let mut metadata_bytes = 0_u64;
    let mut names: HashMap<[u8; 32], NameLocation> = HashMap::new();
    names
        .try_reserve(usize::try_from(directory.entries).unwrap_or(usize::MAX))
        .context("cannot reserve bounded ZIP duplicate-name metadata")?;

    for index in 0..directory.entries {
        checkpoint(execution)?;
        let header_end = cursor.checked_add(HEADER_BYTES).ok_or_else(|| {
            anyhow::anyhow!("{operation}: ZIP central-directory header bounds overflow")
        })?;
        if header_end > directory_end {
            anyhow::bail!(
                "{operation}: ZIP central-directory entry {index} has a truncated header"
            );
        }

        let mut header = [0_u8; HEADER_BYTES as usize];
        reader.seek(SeekFrom::Start(cursor))?;
        reader.read_exact(&mut header)?;
        if &header[..4] != HEADER_SIGNATURE {
            anyhow::bail!("{operation}: ZIP central-directory entry {index} has an invalid header");
        }

        let name_length = le_u16(&header[28..30]);
        if name_length == 0 {
            anyhow::bail!("{operation}: ZIP central-directory entry {index} has an empty name");
        }
        let variable_bytes = u64::from(name_length)
            .checked_add(u64::from(le_u16(&header[30..32])))
            .and_then(|value| value.checked_add(u64::from(le_u16(&header[32..34]))))
            .ok_or_else(|| anyhow::anyhow!("{operation}: ZIP metadata length overflow"))?;
        metadata_bytes = metadata_bytes
            .checked_add(variable_bytes)
            .ok_or_else(|| anyhow::anyhow!("{operation}: cumulative ZIP metadata overflow"))?;
        if metadata_bytes > limits.max_metadata_bytes {
            anyhow::bail!(
                "{operation}: '{}' central-directory file names, extra fields, and comments total \
                 at least {metadata_bytes} bytes, exceeding {} ({})",
                path.display(),
                limits.metadata_limit_name,
                limits.max_metadata_bytes
            );
        }

        cursor = header_end.checked_add(variable_bytes).ok_or_else(|| {
            anyhow::anyhow!("{operation}: ZIP central-directory entry bounds overflow")
        })?;
        if cursor > directory_end {
            anyhow::bail!(
                "{operation}: ZIP central-directory entry {index} extends past its declared bounds"
            );
        }

        let location = NameLocation {
            offset: header_end,
            length: name_length,
        };
        let digest = name_digest(reader, location, execution)?;
        if let Some(previous) = names.get(&digest) {
            if previous.length == location.length
                && names_equal(reader, *previous, location, execution)?
            {
                anyhow::bail!(
                    "{operation}: duplicate archive part name at central-directory entry {index}"
                );
            }
            // Fail closed on the cryptographically improbable case instead of
            // retaining attacker-controlled names to disambiguate it.
            anyhow::bail!("{operation}: ZIP entry-name digest collision at entry {index}");
        }
        names.insert(digest, location);
    }

    checkpoint(execution)?;
    if cursor != directory_end {
        anyhow::bail!(
            "{operation}: ZIP central-directory entries do not exactly fill the declared directory"
        );
    }
    Ok(())
}

fn name_digest<R: Read + Seek>(
    reader: &mut R,
    location: NameLocation,
    execution: Option<&ExecutionControl>,
) -> Result<[u8; 32]> {
    reader.seek(SeekFrom::Start(location.offset))?;
    let mut remaining = usize::from(location.length);
    let mut chunk = [0_u8; NAME_CHUNK_BYTES];
    let mut hasher = Sha256::new();
    while remaining > 0 {
        checkpoint(execution)?;
        let read = remaining.min(chunk.len());
        reader.read_exact(&mut chunk[..read])?;
        hasher.update(&chunk[..read]);
        remaining -= read;
    }
    Ok(hasher.finalize().into())
}

fn names_equal<R: Read + Seek>(
    reader: &mut R,
    left: NameLocation,
    right: NameLocation,
    execution: Option<&ExecutionControl>,
) -> Result<bool> {
    let mut offset = 0_u64;
    let mut left_chunk = [0_u8; NAME_CHUNK_BYTES];
    let mut right_chunk = [0_u8; NAME_CHUNK_BYTES];
    while offset < u64::from(left.length) {
        checkpoint(execution)?;
        let read = usize::try_from((u64::from(left.length) - offset).min(NAME_CHUNK_BYTES as u64))
            .unwrap_or(NAME_CHUNK_BYTES);
        reader.seek(SeekFrom::Start(left.offset + offset))?;
        reader.read_exact(&mut left_chunk[..read])?;
        reader.seek(SeekFrom::Start(right.offset + offset))?;
        reader.read_exact(&mut right_chunk[..read])?;
        if left_chunk[..read] != right_chunk[..read] {
            return Ok(false);
        }
        offset += read as u64;
    }
    Ok(true)
}

fn validate_bounds(
    path: &Path,
    operation: &str,
    directory: Directory,
    limits: Limits<'_>,
) -> Result<()> {
    if directory.entries > limits.max_entries {
        anyhow::bail!(
            "{operation}: '{}' declares {} zip entries, exceeding \
             IRONFLOW_MAX_ZIP_ENTRIES ({})",
            path.display(),
            directory.entries,
            limits.max_entries
        );
    }
    usize::try_from(directory.entries)
        .map_err(|_| anyhow::anyhow!("{operation}: ZIP entry count exceeds this platform"))?;
    if directory.size > limits.max_raw_bytes {
        anyhow::bail!(
            "{operation}: '{}' central directory is {} bytes, exceeding {} ({})",
            path.display(),
            directory.size,
            limits.raw_limit_name,
            limits.max_raw_bytes
        );
    }
    let minimum = directory.entries.saturating_mul(HEADER_BYTES);
    if directory.size < minimum {
        anyhow::bail!("{operation}: ZIP central directory is smaller than its entry count");
    }
    let directory_end = directory
        .offset
        .checked_add(directory.size)
        .ok_or_else(|| anyhow::anyhow!("{operation}: ZIP central-directory bounds overflow"))?;
    if directory_end > directory.end {
        anyhow::bail!("{operation}: ZIP central directory extends outside the archive");
    }
    Ok(())
}

fn checkpoint(execution: Option<&ExecutionControl>) -> Result<()> {
    if let Some(execution) = execution {
        execution.checkpoint()?;
    }
    Ok(())
}
