use uuid::Uuid;

use super::RedisEventStore;
use super::protocol::{
    LEGACY_BATCH_BYTES, LEGACY_BATCH_SIZE, LEGACY_STEPS_PER_OPERATION, LegacyCommit, LegacyFetch,
    LegacyProgress, LegacyStatus, LegacyTransition, MAX_MIGRATION_CONTROL_ATTEMPTS,
};
use super::scripts::{LEGACY_COMMIT, LEGACY_FETCH, LEGACY_RESET, LEGACY_STATUS, LEGACY_TRANSITION};
use crate::storage::redis_config::map_redis_error;
use crate::storage::{StorageError, StorageResult};

impl RedisEventStore {
    pub(super) async fn ensure_layout(&self, run_id: &str) -> StorageResult<()> {
        let proposed_token = Uuid::new_v4().simple().to_string();
        let mut progress = None;
        let mut completed_steps = 0_usize;

        for _ in 0..MAX_MIGRATION_CONTROL_ATTEMPTS {
            let current = match progress.take() {
                Some(progress) => progress,
                None => match self.legacy_status(run_id, &proposed_token).await? {
                    LegacyStatus::Current | LegacyStatus::Empty => return Ok(()),
                    LegacyStatus::Progress(progress) => progress,
                    LegacyStatus::Manual => return Err(manual_migration_required(run_id)),
                    LegacyStatus::Orphaned => return Err(orphaned_snapshot(run_id)),
                    LegacyStatus::Blocked => return Err(blocked_migration(run_id)),
                },
            };

            if current.phase.is_pending()
                || !current.phase.can_fetch()
                || current.cursor == current.sequence
            {
                if should_defer_transition(&current, completed_steps) {
                    return Err(migration_in_progress(run_id, &current));
                }
                let was_pending = current.phase.is_pending();
                match self.legacy_transition(run_id, &current).await? {
                    LegacyTransition::Current => return Ok(()),
                    LegacyTransition::Progress(next) => {
                        if was_pending && !next.phase.is_pending() {
                            completed_steps = completed_steps.saturating_add(1);
                            tokio::task::yield_now().await;
                        }
                        progress = Some(next);
                    }
                    LegacyTransition::Failed(code) => {
                        return Err(legacy_validation_failed(run_id, &code));
                    }
                    LegacyTransition::Expiring => {
                        return Err(StorageError::conflict(format_args!(
                            "Redis legacy event stream for run '{run_id}' is expiring; retry the operation"
                        )));
                    }
                    LegacyTransition::Blocked => return Err(blocked_migration(run_id)),
                    LegacyTransition::Stale => {}
                }
                continue;
            }

            if completed_steps >= LEGACY_STEPS_PER_OPERATION {
                return Err(migration_in_progress(run_id, &current));
            }

            let chunk = match self.fetch_legacy_chunk(run_id, &current).await? {
                LegacyFetch::Chunk(chunk) => chunk,
                LegacyFetch::Invalid(code) => {
                    progress = self.begin_legacy_restore(run_id, &current, &code).await?;
                    continue;
                }
                LegacyFetch::Done | LegacyFetch::Stale => continue,
                LegacyFetch::Blocked => return Err(blocked_migration(run_id)),
            };
            if self
                .validate_legacy_chunk(run_id, &current, &chunk)
                .is_err()
            {
                progress = self
                    .begin_legacy_restore(run_id, &current, "rust_payload")
                    .await?;
                continue;
            }

            match self.commit_legacy_chunk(run_id, &current, &chunk).await? {
                LegacyCommit::Pending | LegacyCommit::Changed | LegacyCommit::Stale => {}
                LegacyCommit::Invalid(code) => {
                    progress = self.begin_legacy_restore(run_id, &current, &code).await?;
                }
                LegacyCommit::Blocked => return Err(blocked_migration(run_id)),
            }
        }

        Err(StorageError::conflict(format_args!(
            "Redis legacy event migration for run '{run_id}' remained busy; retry the operation"
        )))
    }

    async fn legacy_status(&self, run_id: &str, token: &str) -> StorageResult<LegacyStatus> {
        let keys = self.legacy_migration_keys(run_id);
        let mut invocation = keys.prepare(&LEGACY_STATUS);
        let mut conn = self.conn.clone();
        let response = invocation
            .arg(run_id)
            .arg(token)
            .arg(i64::from(Self::uses_encoded_event_keys(run_id)))
            .arg(LEGACY_BATCH_SIZE)
            .arg(LEGACY_BATCH_BYTES)
            .invoke_async::<Vec<String>>(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to inspect Redis event migration for run '{run_id}'"),
                    error,
                )
            })?;
        LegacyStatus::parse(response)
    }

    async fn fetch_legacy_chunk(
        &self,
        run_id: &str,
        progress: &LegacyProgress,
    ) -> StorageResult<LegacyFetch> {
        let keys = self.legacy_migration_keys(run_id);
        let mut invocation = keys.prepare(&LEGACY_FETCH);
        let mut conn = self.conn.clone();
        let response = invocation
            .arg(run_id)
            .arg(&progress.token)
            .arg(progress.generation)
            .arg(progress.phase.as_str())
            .arg(progress.cursor)
            .arg(&progress.digest)
            .invoke_async::<Vec<Vec<u8>>>(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to read a Redis legacy event batch for run '{run_id}'"),
                    error,
                )
            })?;
        LegacyFetch::parse(response)
    }

    async fn commit_legacy_chunk(
        &self,
        run_id: &str,
        progress: &LegacyProgress,
        chunk: &super::protocol::LegacyChunk,
    ) -> StorageResult<LegacyCommit> {
        let keys = self.legacy_migration_keys(run_id);
        let mut invocation = keys.prepare(&LEGACY_COMMIT);
        let mut conn = self.conn.clone();
        let response = invocation
            .arg(run_id)
            .arg(&progress.token)
            .arg(progress.generation)
            .arg(progress.phase.as_str())
            .arg(progress.cursor)
            .arg(&progress.digest)
            .arg(chunk.next_cursor)
            .arg(&chunk.digest)
            .invoke_async::<Vec<String>>(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to rotate a Redis legacy event batch for run '{run_id}'"),
                    error,
                )
            })?;
        LegacyCommit::parse(response)
    }

    async fn legacy_transition(
        &self,
        run_id: &str,
        progress: &LegacyProgress,
    ) -> StorageResult<LegacyTransition> {
        let keys = self.legacy_migration_keys(run_id);
        let mut invocation = keys.prepare(&LEGACY_TRANSITION);
        let mut conn = self.conn.clone();
        let response = invocation
            .arg(run_id)
            .arg(&progress.token)
            .arg(progress.generation)
            .arg(progress.phase.as_str())
            .arg(progress.cursor)
            .arg(&progress.digest)
            .invoke_async::<Vec<String>>(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to advance Redis event migration for run '{run_id}'"),
                    error,
                )
            })?;
        LegacyTransition::parse(response)
    }

    async fn begin_legacy_restore(
        &self,
        run_id: &str,
        progress: &LegacyProgress,
        code: &str,
    ) -> StorageResult<Option<LegacyProgress>> {
        let keys = self.legacy_migration_keys(run_id);
        let mut invocation = keys.prepare(&LEGACY_RESET);
        let mut conn = self.conn.clone();
        let response = invocation
            .arg(run_id)
            .arg(&progress.token)
            .arg(progress.generation)
            .arg(progress.phase.as_str())
            .arg(progress.cursor)
            .arg(&progress.digest)
            .arg(code)
            .invoke_async::<Vec<String>>(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to restore Redis legacy events for run '{run_id}'"),
                    error,
                )
            })?;
        match LegacyTransition::parse(response)? {
            LegacyTransition::Progress(progress) => Ok(Some(progress)),
            LegacyTransition::Stale => Ok(None),
            LegacyTransition::Blocked => Err(blocked_migration(run_id)),
            _ => Err(invalid_protocol_response(
                run_id,
                "restore initialization returned an invalid response",
            )),
        }
    }

    fn validate_legacy_chunk(
        &self,
        run_id: &str,
        progress: &LegacyProgress,
        chunk: &super::protocol::LegacyChunk,
    ) -> StorageResult<()> {
        if chunk.cursor != progress.cursor
            || chunk.next_cursor > progress.sequence
            || chunk.payloads.len() > progress.batch as usize
            || chunk.payloads.iter().map(Vec::len).sum::<usize>() > progress.max_bytes as usize
        {
            return Err(invalid_protocol_response(
                run_id,
                "batch exceeded its persisted migration bounds",
            ));
        }
        for raw in &chunk.payloads {
            Self::decode_event(raw, run_id)?;
        }
        Ok(())
    }
}

fn should_defer_transition(progress: &LegacyProgress, completed_steps: usize) -> bool {
    completed_steps >= LEGACY_STEPS_PER_OPERATION
        && matches!(progress.phase, super::protocol::LegacyPhase::Restore)
        && progress.cursor > 0
}

fn migration_in_progress(run_id: &str, progress: &LegacyProgress) -> StorageError {
    StorageError::conflict(format_args!(
        "Redis legacy event migration for run '{run_id}' made bounded progress through event {} of {}; retry the operation",
        progress.cursor, progress.sequence
    ))
}

fn manual_migration_required(run_id: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Ambiguous Redis legacy event keys for run '{run_id}'"),
        "the physical namespace can alias another run; add an exact run_id owner marker or migrate it manually",
    )
}

fn orphaned_snapshot(run_id: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Orphaned Redis legacy event snapshot for run '{run_id}'"),
        "migration state is missing; the deterministic snapshot was preserved for manual recovery",
    )
}

fn blocked_migration(run_id: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Blocked Redis legacy event migration for run '{run_id}'"),
        "the source, destination, or quarantined snapshot changed; no data was overwritten or deleted",
    )
}

fn legacy_validation_failed(run_id: &str, code: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Invalid stored Redis event for run '{run_id}'"),
        format_args!("legacy validation failed ({code}); the original key family was restored"),
    )
}

fn invalid_protocol_response(run_id: &str, detail: &str) -> StorageError {
    StorageError::corruption(
        format_args!("Invalid Redis legacy event migration for run '{run_id}'"),
        detail,
    )
}
