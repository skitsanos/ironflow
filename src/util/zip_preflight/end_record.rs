//! Selecting the one end record every ZIP parser must agree on.

use std::path::Path;

use anyhow::Result;

use super::fields::le_u16;
use super::{EOCD_BYTES, EOCD_SIGNATURE};

/// Return the index in `tail` of the archive's end record.
///
/// The `zip` reader scans backwards and takes the last signature whose fixed
/// fields and comment fit inside the file, even when bytes follow the comment.
/// A scan that instead wanted the comment to end exactly at end of file could
/// settle on an earlier record and validate a directory the reader never uses
/// (IF-190). This scan therefore stops at the same record the reader tries
/// first and refuses the archive unless that record ends the file. Signature
/// bytes that leave no room for a record, or whose comment length overruns the
/// file, cannot be a record for either parser and stay ordinary comment bytes.
pub(super) fn locate(tail: &[u8], path: &Path, operation: &str) -> Result<usize> {
    for index in (0..=tail.len().saturating_sub(EOCD_BYTES)).rev() {
        if &tail[index..index + 4] != EOCD_SIGNATURE {
            continue;
        }
        let comment_len = usize::from(le_u16(&tail[index + 20..index + 22]));
        let end = index + EOCD_BYTES + comment_len;
        if end > tail.len() {
            continue;
        }
        if end != tail.len() {
            anyhow::bail!(
                "{operation}: '{}' has data after its ZIP end record, or a second end record \
                 inside the archive comment",
                path.display()
            );
        }
        return Ok(index);
    }
    anyhow::bail!(
        "{operation}: '{}' is missing a valid ZIP end record",
        path.display()
    )
}
