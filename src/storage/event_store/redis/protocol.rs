use crate::storage::{StorageError, StorageResult};

pub(super) const LEGACY_BATCH_SIZE: u64 = 128;
pub(super) const LEGACY_BATCH_BYTES: u64 = 1_048_576;
pub(super) const LEGACY_STEPS_PER_OPERATION: usize = 32;
pub(super) const MAX_MIGRATION_CONTROL_ATTEMPTS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LegacyPhase {
    Scan,
    ScanPending,
    Verify,
    VerifyPending,
    Restore,
    RestorePending,
    Finalizing,
}

impl LegacyPhase {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::ScanPending => "scan_pending",
            Self::Verify => "verify",
            Self::VerifyPending => "verify_pending",
            Self::Restore => "restore",
            Self::RestorePending => "restore_pending",
            Self::Finalizing => "finalizing",
        }
    }

    pub(super) const fn is_pending(self) -> bool {
        matches!(
            self,
            Self::ScanPending | Self::VerifyPending | Self::RestorePending
        )
    }

    pub(super) const fn can_fetch(self) -> bool {
        matches!(self, Self::Scan | Self::Verify)
    }

    fn parse(raw: &str) -> StorageResult<Self> {
        match raw {
            "scan" => Ok(Self::Scan),
            "scan_pending" => Ok(Self::ScanPending),
            "verify" => Ok(Self::Verify),
            "verify_pending" => Ok(Self::VerifyPending),
            "restore" => Ok(Self::Restore),
            "restore_pending" => Ok(Self::RestorePending),
            "finalizing" => Ok(Self::Finalizing),
            _ => Err(invalid_response("unknown migration phase")),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct LegacyProgress {
    pub(super) phase: LegacyPhase,
    pub(super) token: String,
    pub(super) generation: u64,
    pub(super) cursor: u64,
    pub(super) sequence: u64,
    pub(super) batch: u64,
    pub(super) max_bytes: u64,
    pub(super) digest: String,
}

#[derive(Debug)]
pub(super) enum LegacyStatus {
    Current,
    Empty,
    Manual,
    Orphaned,
    Blocked,
    Progress(LegacyProgress),
}

impl LegacyStatus {
    pub(super) fn parse(response: Vec<String>) -> StorageResult<Self> {
        match response.first().map(String::as_str) {
            Some("current") if response.len() == 1 => Ok(Self::Current),
            Some("empty") if response.len() == 1 => Ok(Self::Empty),
            Some("manual") if response.len() == 1 => Ok(Self::Manual),
            Some("orphaned") if response.len() == 1 => Ok(Self::Orphaned),
            Some("blocked") if response.len() == 1 => Ok(Self::Blocked),
            Some("progress") => Ok(Self::Progress(parse_progress(&response)?)),
            _ => Err(invalid_response("unexpected status response")),
        }
    }
}

#[derive(Debug)]
pub(super) struct LegacyChunk {
    pub(super) cursor: u64,
    pub(super) next_cursor: u64,
    pub(super) digest: String,
    pub(super) payloads: Vec<Vec<u8>>,
}

#[derive(Debug)]
pub(super) enum LegacyFetch {
    Chunk(LegacyChunk),
    Done,
    Invalid(String),
    Blocked,
    Stale,
}

impl LegacyFetch {
    pub(super) fn parse(mut response: Vec<Vec<u8>>) -> StorageResult<Self> {
        match response.first().map(Vec::as_slice) {
            Some(b"chunk") if response.len() >= 5 => {
                let cursor = parse_uint(response_text(&response[1])?, "chunk cursor")?;
                let next_cursor = parse_uint(response_text(&response[2])?, "chunk next cursor")?;
                if next_cursor <= cursor {
                    return Err(invalid_response("chunk did not advance"));
                }
                let digest = response_text(&response[3])?;
                validate_digest(digest)?;
                let digest = digest.to_string();
                let payloads = response.drain(4..).collect::<Vec<_>>();
                if payloads.is_empty() {
                    return Err(invalid_response("chunk contains no payloads"));
                }
                Ok(Self::Chunk(LegacyChunk {
                    cursor,
                    next_cursor,
                    digest,
                    payloads,
                }))
            }
            Some(b"done") if response.len() == 1 => Ok(Self::Done),
            Some(b"invalid") if response.len() == 2 => {
                let code = response_text(&response[1])?;
                validate_failure_code(code)?;
                Ok(Self::Invalid(code.to_string()))
            }
            Some(b"blocked") if response.len() == 1 => Ok(Self::Blocked),
            Some(b"stale") if response.len() == 1 => Ok(Self::Stale),
            _ => Err(invalid_response("unexpected chunk response")),
        }
    }
}

fn response_text(raw: &[u8]) -> StorageResult<&str> {
    std::str::from_utf8(raw).map_err(|_| invalid_response("non-UTF-8 migration control field"))
}

#[derive(Debug)]
pub(super) enum LegacyCommit {
    Pending,
    Changed,
    Invalid(String),
    Blocked,
    Stale,
}

impl LegacyCommit {
    pub(super) fn parse(response: Vec<String>) -> StorageResult<Self> {
        match response.first().map(String::as_str) {
            Some("pending") if response.len() == 2 => {
                parse_uint(&response[1], "pending generation")?;
                Ok(Self::Pending)
            }
            Some("changed") if response.len() == 1 => Ok(Self::Changed),
            Some("invalid") if response.len() == 2 => {
                validate_failure_code(&response[1])?;
                Ok(Self::Invalid(response[1].clone()))
            }
            Some("blocked") if response.len() == 1 => Ok(Self::Blocked),
            Some("stale") if response.len() == 1 => Ok(Self::Stale),
            _ => Err(invalid_response("unexpected commit response")),
        }
    }
}

#[derive(Debug)]
pub(super) enum LegacyTransition {
    Current,
    Progress(LegacyProgress),
    Failed(String),
    Expiring,
    Blocked,
    Stale,
}

impl LegacyTransition {
    pub(super) fn parse(response: Vec<String>) -> StorageResult<Self> {
        match response.first().map(String::as_str) {
            Some("current") if response.len() == 1 => Ok(Self::Current),
            Some("progress") => Ok(Self::Progress(parse_progress(&response)?)),
            Some("failed") if response.len() == 2 => {
                validate_failure_code(&response[1])?;
                Ok(Self::Failed(response[1].clone()))
            }
            Some("expiring") if response.len() == 1 => Ok(Self::Expiring),
            Some("blocked") if response.len() == 1 => Ok(Self::Blocked),
            Some("stale") if response.len() == 1 => Ok(Self::Stale),
            _ => Err(invalid_response("unexpected transition response")),
        }
    }
}

fn parse_progress(response: &[String]) -> StorageResult<LegacyProgress> {
    if response.len() != 11 {
        return Err(invalid_response("incomplete progress response"));
    }
    let phase = LegacyPhase::parse(&response[1])?;
    validate_token(&response[2])?;
    if response[3] != "current" && response[3] != "raw" {
        return Err(invalid_response("unknown source mode"));
    }
    let generation = parse_uint(&response[4], "progress generation")?;
    let cursor = parse_uint(&response[5], "progress cursor")?;
    let sequence = parse_uint(&response[6], "progress sequence")?;
    if cursor > sequence {
        return Err(invalid_response("progress cursor exceeds sequence"));
    }
    let batch = parse_uint(&response[7], "progress batch")?;
    let max_bytes = parse_uint(&response[8], "progress byte limit")?;
    if !(1..=LEGACY_BATCH_SIZE).contains(&batch) || !(1..=LEGACY_BATCH_BYTES).contains(&max_bytes) {
        return Err(invalid_response("unsupported migration policy"));
    }
    validate_digest(&response[9])?;
    if response[10] != "-" {
        validate_digest(&response[10])?;
    }
    Ok(LegacyProgress {
        phase,
        token: response[2].clone(),
        generation,
        cursor,
        sequence,
        batch,
        max_bytes,
        digest: response[9].clone(),
    })
}

fn parse_uint(raw: &str, label: &str) -> StorageResult<u64> {
    raw.parse::<u64>()
        .map_err(|_| invalid_response(format_args!("invalid {label}")))
}

fn validate_token(token: &str) -> StorageResult<()> {
    if token.len() != 32 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_response("invalid migration token"));
    }
    Ok(())
}

fn validate_digest(digest: &str) -> StorageResult<()> {
    if digest.len() != 40
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid_response("invalid migration digest"));
    }
    Ok(())
}

fn validate_failure_code(code: &str) -> StorageResult<()> {
    if code.is_empty()
        || code.len() > 64
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
        return Err(invalid_response("invalid migration failure code"));
    }
    Ok(())
}

fn invalid_response(detail: impl std::fmt::Display) -> StorageError {
    StorageError::corruption("Invalid Redis legacy event migration response", detail)
}
