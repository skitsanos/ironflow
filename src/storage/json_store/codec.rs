use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::engine::types::{RunInfo, RunSummary};
use crate::storage::run_id::validate_run_id;
use crate::storage::{StorageError, StorageResult};

pub(super) const REVISION_PREFIX_BYTES: usize = 512;

const REVISION_FIELD: &str = "_ironflow_revision";
const SUMMARY_DIGEST_FIELD: &str = "_ironflow_summary_digest";

pub(super) struct EncodedRecord {
    pub revision: String,
    pub run: Vec<u8>,
    pub summary: Vec<u8>,
}

pub(super) struct DecodedRun {
    pub revision: Option<String>,
    pub summary_digest: Option<String>,
    pub info: RunInfo,
}

pub(super) struct DecodedSummary {
    pub revision: Option<String>,
    pub digest: Option<String>,
    pub summary: RunSummary,
}

pub(super) struct PrimarySummaryHeader {
    pub revision: String,
    pub digest: String,
}

#[derive(Serialize)]
struct RevisionedRun<'a> {
    #[serde(rename = "_ironflow_revision")]
    revision: &'a str,
    #[serde(rename = "_ironflow_summary_digest")]
    summary_digest: &'a str,
    #[serde(flatten)]
    info: &'a RunInfo,
}

#[derive(Serialize)]
struct RevisionedSummary<'a> {
    #[serde(rename = "_ironflow_revision")]
    revision: &'a str,
    #[serde(rename = "_ironflow_summary_digest")]
    summary_digest: &'a str,
    #[serde(flatten)]
    summary: &'a RunSummary,
}

#[derive(Deserialize)]
struct StoredRun {
    #[serde(rename = "_ironflow_revision", default)]
    revision: Option<String>,
    #[serde(rename = "_ironflow_summary_digest", default)]
    summary_digest: Option<String>,
    #[serde(flatten)]
    info: RunInfo,
}

pub(super) fn validate_input_id(run_id: &str) -> StorageResult<()> {
    validate_run_id(run_id)
        .map_err(|error| StorageError::invalid_input(format_args!("Invalid run ID: {error}")))
}

pub(super) fn validate_filename_id(run_id: &str, name: &str) -> StorageResult<()> {
    validate_run_id(run_id).map_err(|error| {
        StorageError::corruption(format_args!("Invalid JSON store filename '{name}'"), error)
    })
}

pub(super) fn encode_record(info: &RunInfo, run_id: &str) -> StorageResult<EncodedRecord> {
    let revision = Uuid::new_v4().to_string();
    let summary = RunSummary::from(info);
    let summary_digest = encode_summary_digest(&summary, run_id)?;
    let run = serde_json::to_vec_pretty(&RevisionedRun {
        revision: &revision,
        summary_digest: &summary_digest,
        info,
    })
    .map_err(|error| {
        StorageError::backend(format_args!("Failed to serialize run '{run_id}'"), error)
    })?;
    let summary = encode_summary_with_digest(&revision, &summary_digest, &summary, run_id)?;
    Ok(EncodedRecord {
        revision,
        run,
        summary,
    })
}

pub(super) fn encode_summary(
    revision: &str,
    summary: &RunSummary,
    run_id: &str,
) -> StorageResult<Vec<u8>> {
    let summary_digest = encode_summary_digest(summary, run_id)?;
    encode_summary_with_digest(revision, &summary_digest, summary, run_id)
}

fn encode_summary_with_digest(
    revision: &str,
    summary_digest: &str,
    summary: &RunSummary,
    run_id: &str,
) -> StorageResult<Vec<u8>> {
    serde_json::to_vec(&RevisionedSummary {
        revision,
        summary_digest,
        summary,
    })
    .map_err(|error| {
        StorageError::backend(
            format_args!("Failed to serialize summary for run '{run_id}'"),
            error,
        )
    })
}

pub(super) fn decode_run(data: &[u8], run_id: &str, name: &str) -> StorageResult<DecodedRun> {
    let stored: StoredRun = serde_json::from_slice(data).map_err(|error| {
        StorageError::corruption(format_args!("Failed to parse run '{run_id}'"), error)
    })?;
    validate_stored_id(&stored.info.id, run_id, name)?;
    if stored
        .revision
        .as_deref()
        .is_some_and(|revision| !is_canonical_revision(revision))
    {
        return Err(StorageError::corruption(
            format_args!("Invalid JSON store revision in '{name}'"),
            "revision must be a canonical UUID",
        ));
    }
    match (&stored.revision, &stored.summary_digest) {
        (None, None) | (Some(_), None) => {}
        (None, Some(_)) => {
            return Err(StorageError::corruption(
                format_args!("Invalid JSON store summary digest in '{name}'"),
                "a summary digest requires a revision",
            ));
        }
        (Some(_), Some(digest)) => {
            if !is_canonical_digest(digest) {
                return Err(StorageError::corruption(
                    format_args!("Invalid JSON store summary digest in '{name}'"),
                    "digest must be a lowercase SHA-256 value",
                ));
            }
            let actual = encode_summary_digest(&RunSummary::from(&stored.info), run_id).map_err(
                |error| {
                    StorageError::corruption(
                        format_args!("Invalid JSON store summary digest in '{name}'"),
                        error,
                    )
                },
            )?;
            if digest != &actual {
                return Err(StorageError::corruption(
                    format_args!("Invalid JSON store summary digest in '{name}'"),
                    "digest does not match the authoritative primary summary",
                ));
            }
        }
    }
    Ok(DecodedRun {
        revision: stored.revision,
        summary_digest: stored.summary_digest,
        info: stored.info,
    })
}

pub(super) fn decode_summary(
    data: &[u8],
    run_id: &str,
    name: &str,
) -> StorageResult<Option<DecodedSummary>> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return Ok(None);
    };
    let Some(stored_id) = value.get("id").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    validate_stored_id(stored_id, run_id, name)?;
    let revision = decode_cache_revision(&value);
    let digest = decode_cache_digest(&value);
    Ok(serde_json::from_value::<RunSummary>(value)
        .ok()
        .map(|summary| DecodedSummary {
            revision,
            digest,
            summary,
        }))
}

/// Decode the revision and identity from the bounded prefix emitted by
/// `encode_record`. Any legacy or reformatted record returns `None` and is
/// decoded through the authoritative full-record path instead.
pub(super) fn decode_revision_prefix(data: &[u8], run_id: &str) -> Option<PrimarySummaryHeader> {
    let text = std::str::from_utf8(data).ok()?;
    let mut rest = text.trim_start().strip_prefix('{')?.trim_start();
    rest = rest.strip_prefix("\"_ironflow_revision\"")?;
    rest = rest.trim_start().strip_prefix(':')?;
    let (revision, remaining) = take_json_string(rest)?;
    let mut rest = remaining.trim_start().strip_prefix(',')?.trim_start();
    rest = rest.strip_prefix("\"_ironflow_summary_digest\"")?;
    rest = rest.trim_start().strip_prefix(':')?;
    let (digest, remaining) = take_json_string(rest)?;
    let mut rest = remaining.trim_start().strip_prefix(',')?.trim_start();
    rest = rest.strip_prefix("\"id\"")?;
    rest = rest.trim_start().strip_prefix(':')?;
    let (stored_id, _) = take_json_string(rest)?;
    if stored_id != run_id || !is_canonical_revision(&revision) || !is_canonical_digest(&digest) {
        return None;
    }
    Some(PrimarySummaryHeader { revision, digest })
}

pub(super) fn summary_matches_digest(
    summary: &RunSummary,
    expected: &str,
    run_id: &str,
) -> StorageResult<bool> {
    Ok(encode_summary_digest(summary, run_id)? == expected)
}

pub(super) fn managed_entry(name: &str) -> Option<(&str, bool)> {
    if let Some(run_id) = name.strip_suffix(".summary.json") {
        Some((run_id, true))
    } else {
        name.strip_suffix(".json").map(|run_id| (run_id, false))
    }
}

fn validate_stored_id(stored: &str, expected: &str, entry: &str) -> StorageResult<()> {
    validate_run_id(stored).map_err(|error| {
        StorageError::corruption(format_args!("Invalid run ID stored in '{entry}'"), error)
    })?;
    if stored != expected {
        return Err(StorageError::corruption(
            format_args!("Invalid run identity stored in '{entry}'"),
            "payload ID does not match its filename",
        ));
    }
    Ok(())
}

fn decode_cache_revision(value: &serde_json::Value) -> Option<String> {
    let revision = value.get(REVISION_FIELD)?.as_str()?;
    is_canonical_revision(revision).then(|| revision.to_string())
}

fn decode_cache_digest(value: &serde_json::Value) -> Option<String> {
    let digest = value.get(SUMMARY_DIGEST_FIELD)?.as_str()?;
    is_canonical_digest(digest).then(|| digest.to_string())
}

fn is_canonical_revision(revision: &str) -> bool {
    Uuid::parse_str(revision).is_ok_and(|parsed| parsed.to_string() == revision)
}

fn is_canonical_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn encode_summary_digest(summary: &RunSummary, run_id: &str) -> StorageResult<String> {
    let payload = serde_json::to_vec(summary).map_err(|error| {
        StorageError::backend(
            format_args!("Failed to serialize summary digest for run '{run_id}'"),
            error,
        )
    })?;
    Ok(hex::encode(Sha256::digest(payload)))
}

fn take_json_string(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    let mut values = serde_json::Deserializer::from_str(input).into_iter::<String>();
    let value = values.next()?.ok()?;
    let offset = values.byte_offset();
    Some((value, &input[offset..]))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Utc;

    use super::*;
    use crate::engine::types::RunStatus;

    fn info() -> RunInfo {
        RunInfo {
            id: "revision-probe".to_string(),
            flow_name: "flow".to_string(),
            status: RunStatus::Pending,
            started: Some(Utc::now()),
            finished: None,
            ctx: HashMap::new(),
            tasks: HashMap::new(),
        }
    }

    #[test]
    fn encoded_primary_exposes_a_bounded_revision_header() {
        let encoded = encode_record(&info(), "revision-probe").unwrap();
        let prefix = &encoded.run[..encoded.run.len().min(REVISION_PREFIX_BYTES)];

        assert_eq!(
            decode_revision_prefix(prefix, "revision-probe")
                .as_ref()
                .map(|header| header.revision.as_str()),
            Some(encoded.revision.as_str())
        );
        assert_eq!(
            decode_run(&encoded.run, "revision-probe", "revision-probe.json")
                .unwrap()
                .revision
                .as_deref(),
            Some(encoded.revision.as_str())
        );
    }

    #[test]
    fn bounded_revision_header_includes_the_longest_valid_run_id() {
        let run_id = "a".repeat(crate::storage::MAX_RUN_ID_BYTES);
        let mut info = info();
        info.id.clone_from(&run_id);
        let encoded = encode_record(&info, &run_id).unwrap();
        let prefix = &encoded.run[..encoded.run.len().min(REVISION_PREFIX_BYTES)];

        assert_eq!(
            decode_revision_prefix(prefix, &run_id)
                .as_ref()
                .map(|header| header.revision.as_str()),
            Some(encoded.revision.as_str())
        );
    }

    #[test]
    fn legacy_primary_has_no_revision_header() {
        let data = serde_json::to_vec_pretty(&info()).unwrap();
        assert!(decode_revision_prefix(&data, "revision-probe").is_none());
        assert!(
            decode_run(&data, "revision-probe", "revision-probe.json")
                .unwrap()
                .revision
                .is_none()
        );
    }
}
