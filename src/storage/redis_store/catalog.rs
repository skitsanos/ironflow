use std::sync::LazyLock;

use super::RedisStateStore;
use super::atomic::LEGACY_REVISION;
use crate::engine::types::RunSummary;
use crate::storage::redis_config::map_redis_error;
use crate::storage::{StorageError, StorageResult};

const CATALOG_UPSERT_SCRIPT_SOURCE: &str = include_str!("scripts/catalog_upsert.lua");

static CATALOG_UPSERT_SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(CATALOG_UPSERT_SCRIPT_SOURCE));

pub(super) struct CatalogSnapshot {
    summary: RunSummary,
    revision: String,
}

pub(super) enum CatalogUpsert {
    Updated,
    Missing,
    Conflict,
}

impl RedisStateStore {
    pub(super) async fn read_catalog_snapshot(
        &self,
        run_id: &str,
    ) -> StorageResult<Option<CatalogSnapshot>> {
        let key = self.resolve_run_key(run_id).await?;
        let mut conn = self.conn.clone();
        let (raw_summary, revision): (Option<String>, Option<String>) = redis::cmd("HMGET")
            .arg(&key)
            .arg("summary")
            .arg("revision")
            .query_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to read Redis summary for run '{run_id}'"),
                    error,
                )
            })?;

        if let Some(raw_summary) = raw_summary {
            let summary: RunSummary = serde_json::from_str(&raw_summary).map_err(|error| {
                StorageError::corruption(
                    format_args!("Failed to parse Redis summary for run '{run_id}'"),
                    error,
                )
            })?;
            if summary.id != run_id {
                return Err(StorageError::corruption(
                    format_args!("Invalid Redis summary for run '{run_id}'"),
                    "stored run-summary identity does not match its key",
                ));
            }
            return Ok(Some(CatalogSnapshot {
                summary,
                revision: revision.unwrap_or_else(|| LEGACY_REVISION.to_string()),
            }));
        }

        match self.read_snapshot(run_id).await {
            Ok(snapshot) => Ok(Some(CatalogSnapshot {
                summary: RunSummary::from(&snapshot.info),
                revision: snapshot.revision,
            })),
            Err(error) if error.is_not_found() => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn upsert_ordered_catalog_entry(
        &self,
        run_id: &str,
        snapshot: &CatalogSnapshot,
    ) -> StorageResult<CatalogUpsert> {
        let run_key = self.resolve_run_key(run_id).await?;
        let raw_summary = serde_json::to_string(&snapshot.summary).map_err(|error| {
            StorageError::backend(
                format_args!("Failed to serialize Redis run summary '{run_id}'"),
                error,
            )
        })?;
        let ordered_member = super::listing::ordered_member(&snapshot.summary);
        let status_keys = self.ordered_status_keys();
        let mut conn = self.conn.clone();
        let result: i64 = CATALOG_UPSERT_SCRIPT
            .key(run_key)
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
            .arg(&raw_summary)
            .arg(run_id)
            .arg(&ordered_member)
            .arg(snapshot.summary.status.to_string())
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to update Redis ordered catalog for run '{run_id}'"),
                    error,
                )
            })?;
        match result {
            1 => Ok(CatalogUpsert::Updated),
            0 => Ok(CatalogUpsert::Missing),
            2 => Ok(CatalogUpsert::Conflict),
            other => Err(StorageError::corruption(
                format_args!("Invalid Redis catalog result for run '{run_id}'"),
                other,
            )),
        }
    }
}
