use std::sync::LazyLock;
use std::time::Duration;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::RedisStateStore;
use crate::engine::types::{RunInfo, RunSummary};
use crate::storage::redis_config::map_redis_error;
use crate::storage::{StorageError, StorageResult};

pub(super) const LEGACY_REVISION: &str = "__ironflow_legacy_revision__";
const INITIAL_CAS_BACKOFF_MICROS: u64 = 50;
const MAX_CAS_BACKOFF_MICROS: u64 = 50_000;

const INIT_SCRIPT_SOURCE: &str = include_str!("scripts/init.lua");
const CAS_SCRIPT_SOURCE: &str = include_str!("scripts/cas.lua");
const DELETE_SCRIPT_SOURCE: &str = include_str!("scripts/delete.lua");
const MIGRATE_SCRIPT_SOURCE: &str = include_str!("scripts/migrate.lua");
const SWEEP_SCRIPT_SOURCE: &str = include_str!("scripts/sweep.lua");

static INIT_SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(INIT_SCRIPT_SOURCE));
static CAS_SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(CAS_SCRIPT_SOURCE));
static DELETE_SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(DELETE_SCRIPT_SOURCE));
static MIGRATE_SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(MIGRATE_SCRIPT_SOURCE));
static SWEEP_SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(SWEEP_SCRIPT_SOURCE));

pub(super) struct RunSnapshot {
    pub(super) info: RunInfo,
    pub(super) revision: String,
    incarnation: String,
}

fn legacy_incarnation(info: &RunInfo) -> StorageResult<String> {
    let immutable_identity = serde_json::to_vec(&(&info.id, &info.flow_name, info.started))
        .map_err(|error| {
            StorageError::backend(
                format_args!("Failed to serialize Redis run identity '{}'", info.id),
                error,
            )
        })?;
    Ok(format!(
        "legacy:{}",
        hex::encode(Sha256::digest(immutable_identity))
    ))
}

async fn backoff_after_conflict(conflicts: u32) {
    let exponent = conflicts.saturating_sub(1).min(10);
    let ceiling_micros = INITIAL_CAS_BACKOFF_MICROS
        .saturating_mul(1_u64 << exponent)
        .min(MAX_CAS_BACKOFF_MICROS);
    let jitter = (Uuid::new_v4().as_u128() % u128::from(ceiling_micros)) as u64 + 1;
    tokio::time::sleep(Duration::from_micros(jitter)).await;
}

impl RedisStateStore {
    pub(super) async fn resolve_run_key(&self, run_id: &str) -> StorageResult<String> {
        let current_key = self.run_key(run_id);
        if crate::storage::redis_keys::is_legacy_safe_run_id(run_id) {
            return Ok(current_key);
        }

        let legacy_key = format!("{}runs:{run_id}", self.prefix);
        let mut conn = self.conn.clone();
        let _: i64 = MIGRATE_SCRIPT
            .key(&current_key)
            .key(&legacy_key)
            .key(self.index_key())
            .arg(run_id)
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to migrate Redis key for run '{run_id}'"),
                    error,
                )
            })?;
        Ok(current_key)
    }

    pub(super) async fn read_snapshot(&self, run_id: &str) -> StorageResult<RunSnapshot> {
        let key = self.resolve_run_key(run_id).await?;
        let mut conn = self.conn.clone();
        let (raw_info, revision, incarnation): (Option<String>, Option<String>, Option<String>) =
            redis::cmd("HMGET")
                .arg(&key)
                .arg("info")
                .arg("revision")
                .arg("incarnation")
                .query_async(&mut conn)
                .await
                .map_err(|error| {
                    map_redis_error(format_args!("Failed to read Redis run '{run_id}'"), error)
                })?;
        let raw_info = match raw_info {
            Some(raw_info) => raw_info,
            None => {
                let key_type: String = redis::cmd("TYPE")
                    .arg(&key)
                    .query_async(&mut conn)
                    .await
                    .map_err(|error| {
                        map_redis_error(
                            format_args!("Failed to inspect Redis run '{run_id}'"),
                            error,
                        )
                    })?;
                if key_type == "none" {
                    return Err(StorageError::not_found(format_args!(
                        "Run '{run_id}' not found"
                    )));
                }
                return Err(StorageError::corruption(
                    format_args!("Invalid Redis run '{run_id}'"),
                    "run hash is missing its info field",
                ));
            }
        };
        let info: RunInfo = serde_json::from_str(&raw_info).map_err(|error| {
            StorageError::corruption(format_args!("Failed to parse Redis run '{run_id}'"), error)
        })?;
        if info.id != run_id {
            return Err(StorageError::corruption(
                format_args!("Invalid Redis run '{run_id}'"),
                "stored run identity does not match its key",
            ));
        }

        let incarnation = match incarnation {
            Some(incarnation) => incarnation,
            None => legacy_incarnation(&info)?,
        };

        Ok(RunSnapshot {
            info,
            revision: revision.unwrap_or_else(|| LEGACY_REVISION.to_string()),
            incarnation,
        })
    }

    pub(super) async fn initialize_run(&self, info: &RunInfo) -> StorageResult<()> {
        let run_key = self.resolve_run_key(&info.id).await?;
        let mut conn = self.conn.clone();
        let index_key = self.index_key();
        let raw_info = serde_json::to_string(info).map_err(|error| {
            StorageError::backend(
                format_args!("Failed to serialize Redis run '{}'", info.id),
                error,
            )
        })?;
        let raw_summary = serde_json::to_string(&RunSummary::from(info)).map_err(|error| {
            StorageError::backend(
                format_args!("Failed to serialize Redis run summary '{}'", info.id),
                error,
            )
        })?;
        let revision = Uuid::new_v4().simple().to_string();
        let incarnation = Uuid::new_v4().simple().to_string();
        let ordered_member = super::listing::ordered_member(&RunSummary::from(info));
        let status_keys = self.ordered_status_keys();

        let initialized: i64 = INIT_SCRIPT
            .key(&run_key)
            .key(&index_key)
            .key(self.ordered_catalog_members_key())
            .key(self.ordered_catalog_key())
            .key(&status_keys[0])
            .key(&status_keys[1])
            .key(&status_keys[2])
            .key(&status_keys[3])
            .key(&status_keys[4])
            .key(&status_keys[5])
            .key(self.ordered_catalog_ready_key())
            .arg(&raw_info)
            .arg(&raw_summary)
            .arg(&revision)
            .arg(&incarnation)
            .arg(&info.id)
            .arg(self.ttl.unwrap_or(-1))
            .arg(&ordered_member)
            .arg(info.status.to_string())
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to initialize Redis run '{}'", info.id),
                    error,
                )
            })?;
        match initialized {
            1 => Ok(()),
            0 => Err(StorageError::conflict(format_args!(
                "Run '{}' already exists",
                info.id
            ))),
            _ => Err(StorageError::corruption(
                format_args!("Invalid initialization result for Redis run '{}'", info.id),
                initialized,
            )),
        }
    }

    pub(super) async fn mutate_run<F>(&self, run_id: &str, mutate: F) -> StorageResult<()>
    where
        F: Fn(&mut RunInfo) -> StorageResult<bool>,
    {
        let mut snapshot = self.read_snapshot(run_id).await?;
        let incarnation = snapshot.incarnation.clone();
        let mut conflicts = 0_u32;
        loop {
            if snapshot.incarnation != incarnation {
                return Err(StorageError::conflict(format_args!(
                    "Run '{run_id}' was deleted and recreated during a Redis mutation"
                )));
            }
            if !mutate(&mut snapshot.info)? {
                return Ok(());
            }

            let raw_info = serde_json::to_string(&snapshot.info).map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to serialize Redis run '{run_id}'"),
                    error,
                )
            })?;
            let raw_summary =
                serde_json::to_string(&RunSummary::from(&snapshot.info)).map_err(|error| {
                    StorageError::backend(
                        format_args!("Failed to serialize Redis run summary '{run_id}'"),
                        error,
                    )
                })?;
            let next_revision = Uuid::new_v4().simple().to_string();
            let summary = RunSummary::from(&snapshot.info);
            let ordered_member = super::listing::ordered_member(&summary);
            let status_keys = self.ordered_status_keys();
            let mut conn = self.conn.clone();
            let swapped: i64 = CAS_SCRIPT
                .key(self.run_key(run_id))
                .key(self.ordered_catalog_members_key())
                .key(self.ordered_catalog_key())
                .key(&status_keys[0])
                .key(&status_keys[1])
                .key(&status_keys[2])
                .key(&status_keys[3])
                .key(&status_keys[4])
                .key(&status_keys[5])
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
                .invoke_async(&mut conn)
                .await
                .map_err(|error| {
                    map_redis_error(format_args!("Failed to mutate Redis run '{run_id}'"), error)
                })?;
            if swapped == 1 {
                return Ok(());
            }
            if swapped == -1 {
                return Err(StorageError::conflict(format_args!(
                    "Run '{run_id}' was deleted and recreated during a Redis mutation"
                )));
            }
            if swapped != 0 {
                return Err(StorageError::corruption(
                    format_args!("Invalid mutation result for Redis run '{run_id}'"),
                    swapped,
                ));
            }
            // A revision conflict proves that another writer committed. Keep
            // retrying while the incarnation is stable instead of converting
            // healthy forward progress into an arbitrary retry-limit failure.
            conflicts = conflicts.saturating_add(1);
            backoff_after_conflict(conflicts).await;
            snapshot = self.read_snapshot(run_id).await?;
        }
    }

    pub(super) async fn delete_run_atomic(&self, run_id: &str) -> StorageResult<()> {
        let run_key = self.resolve_run_key(run_id).await?;
        let mut conn = self.conn.clone();
        let status_keys = self.ordered_status_keys();
        let deleted: i64 = DELETE_SCRIPT
            .key(run_key)
            .key(self.index_key())
            .key(self.ordered_catalog_members_key())
            .key(self.ordered_catalog_key())
            .key(&status_keys[0])
            .key(&status_keys[1])
            .key(&status_keys[2])
            .key(&status_keys[3])
            .key(&status_keys[4])
            .key(&status_keys[5])
            .arg(run_id)
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(format_args!("Failed to delete Redis run '{run_id}'"), error)
            })?;
        match deleted {
            1 => Ok(()),
            0 => Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            ))),
            _ => Err(StorageError::corruption(
                format_args!("Invalid deletion result for Redis run '{run_id}'"),
                deleted,
            )),
        }
    }

    pub(super) async fn remove_stale_index_entry(&self, run_id: &str) -> StorageResult<bool> {
        let run_key = self.resolve_run_key(run_id).await?;
        let mut conn = self.conn.clone();
        let status_keys = self.ordered_status_keys();
        let removed: i64 = SWEEP_SCRIPT
            .key(run_key)
            .key(self.index_key())
            .key(self.ordered_catalog_members_key())
            .key(self.ordered_catalog_key())
            .key(&status_keys[0])
            .key(&status_keys[1])
            .key(&status_keys[2])
            .key(&status_keys[3])
            .key(&status_keys[4])
            .key(&status_keys[5])
            .arg(run_id)
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to sweep Redis index for run '{run_id}'"),
                    error,
                )
            })?;
        match removed {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(StorageError::corruption(
                format_args!("Invalid sweep result for Redis run '{run_id}'"),
                removed,
            )),
        }
    }
}
