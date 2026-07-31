use std::sync::LazyLock;

use uuid::Uuid;

use super::RedisStateStore;
use super::atomic::{LEGACY_REVISION, backoff_after_conflict};
use crate::engine::types::{RunInfo, RunSummary};
use crate::storage::redis_config::map_redis_error;
use crate::storage::{StorageError, StorageResult};

const STATUS_SOURCE: &str = include_str!("scripts/lease_status.lua");
static STATUS_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| redis::Script::new(STATUS_SOURCE));

#[derive(Clone, Copy)]
pub(super) enum LeaseGuard<'a> {
    Owner(&'a str),
    Expired,
}

impl RedisStateStore {
    /// CAS one complete run projection while the same Lua invocation verifies
    /// an unexpired owner or claims an expired lease. Terminal mutations
    /// release the lease and its expiry-index member atomically.
    pub(super) async fn guarded_mutation<T, F>(
        &self,
        run_id: &str,
        guard: LeaseGuard<'_>,
        mutate: F,
    ) -> StorageResult<Option<T>>
    where
        F: Fn(&mut RunInfo) -> T,
    {
        let mut snapshot = self.read_snapshot(run_id).await?;
        let incarnation = snapshot.incarnation.clone();
        let mut conflicts = 0_u32;
        loop {
            if snapshot.incarnation != incarnation {
                return Err(StorageError::conflict(format_args!(
                    "Run '{run_id}' was deleted and recreated during lease fencing"
                )));
            }
            let result = mutate(&mut snapshot.info);
            let raw_info = serde_json::to_string(&snapshot.info).map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to serialize Redis run '{run_id}'"),
                    error,
                )
            })?;
            let summary = RunSummary::from(&snapshot.info);
            let raw_summary = serde_json::to_string(&summary).map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to serialize Redis run summary '{run_id}'"),
                    error,
                )
            })?;
            let next_revision = Uuid::new_v4().simple().to_string();
            let ordered_member = super::listing::ordered_member(&summary);
            let (guard_name, guard_value) = match guard {
                LeaseGuard::Owner(owner) => ("owner", owner),
                LeaseGuard::Expired => ("expired", ""),
            };
            let status_keys = self.ordered_status_keys();
            let mut conn = self.conn.clone();
            let swapped: i64 = STATUS_SCRIPT
                .key(self.run_key(run_id))
                .key(self.ordered_catalog_members_key())
                .key(self.ordered_catalog_key())
                .key(&status_keys[0])
                .key(&status_keys[1])
                .key(&status_keys[2])
                .key(&status_keys[3])
                .key(&status_keys[4])
                .key(&status_keys[5])
                .key(self.run_lease_expiry_key())
                .arg(LEGACY_REVISION)
                .arg(&snapshot.revision)
                .arg(&incarnation)
                .arg(&raw_info)
                .arg(&raw_summary)
                .arg(self.ttl.unwrap_or(-1))
                .arg(&next_revision)
                .arg(run_id)
                .arg(&ordered_member)
                .arg(summary.status.to_string())
                .arg(guard_name)
                .arg(guard_value)
                .arg(if summary.status.is_terminal() { 1 } else { 0 })
                .arg(crate::storage::run_lease::RUN_LEASE_KEY_SAFETY.as_micros())
                .invoke_async(&mut conn)
                .await
                .map_err(|error| {
                    map_redis_error(
                        format_args!("Failed to fence Redis run lease '{run_id}'"),
                        error,
                    )
                })?;
            match swapped {
                1 => return Ok(Some(result)),
                -2 | -3 => return Ok(None),
                -1 => {
                    return Err(StorageError::conflict(format_args!(
                        "Run '{run_id}' was deleted and recreated during lease fencing"
                    )));
                }
                0 => {
                    conflicts = conflicts.saturating_add(1);
                    backoff_after_conflict(conflicts).await;
                    snapshot = self.read_snapshot(run_id).await?;
                }
                value => {
                    return Err(StorageError::corruption(
                        format_args!("Invalid Redis run lease CAS result for '{run_id}'"),
                        value,
                    ));
                }
            }
        }
    }
}
