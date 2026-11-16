//! Errors reported by the Redis legacy-event migration.

use super::protocol::LegacyProgress;
use crate::storage::StorageError;

pub(super) fn migration_in_progress(run_id: &str, progress: &LegacyProgress) -> StorageError {
    StorageError::conflict(format_args!(
        "Redis legacy event migration for run '{run_id}' made bounded progress through event {} of {}; retry the operation",
        progress.cursor, progress.sequence
    ))
}

pub(super) fn manual_migration_required(run_id: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Ambiguous Redis legacy event keys for run '{run_id}'"),
        "the physical namespace can alias another run; add an exact run_id owner marker or migrate it manually",
    )
}

pub(super) fn orphaned_snapshot(run_id: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Orphaned Redis legacy event snapshot for run '{run_id}'"),
        "migration state is missing; the deterministic snapshot was preserved for manual recovery",
    )
}

pub(super) fn blocked_migration(run_id: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Blocked Redis legacy event migration for run '{run_id}'"),
        "the source, destination, or quarantined snapshot changed; no data was overwritten or deleted",
    )
}

pub(super) fn legacy_validation_failed(run_id: &str, code: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Invalid stored Redis event for run '{run_id}'"),
        format_args!("legacy validation failed ({code}); the original key family was restored"),
    )
}

pub(super) fn invalid_protocol_response(run_id: &str, detail: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Invalid Redis legacy event migration for run '{run_id}'"),
        detail,
    )
}

/// The quarantined stream expired while it was being migrated. Nothing is
/// corrupt: once its captured deadline has passed the stream reads as empty
/// (IF-214).
pub(super) fn stream_expiring(run_id: &str) -> StorageError {
    StorageError::conflict(format_args!(
        "Redis legacy event stream for run '{run_id}' is expiring; retry the operation"
    ))
}
