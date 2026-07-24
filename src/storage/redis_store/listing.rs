use std::collections::HashSet;

use chrono::{DateTime, Utc};

use super::RedisStateStore;
use super::catalog::{CatalogSnapshot, CatalogUpsert};
use crate::engine::types::RunSummary;
use crate::storage::redis_config::map_redis_error;
use crate::storage::run_listing::normalized_started;
use crate::storage::{RunListQuery, RunSummaryPage, StorageError, StorageErrorKind, StorageResult};

pub(super) fn ordered_member(summary: &RunSummary) -> String {
    ordered_member_parts(summary.started, &summary.id)
}

fn ordered_member_parts(started: Option<DateTime<Utc>>, run_id: &str) -> String {
    match normalized_started(started) {
        Some(micros) => {
            let sortable = (micros as u64) ^ (1_u64 << 63);
            format!("1:{sortable:016x}:{run_id}")
        }
        None => format!("0:{run_id}"),
    }
}

pub(super) fn run_id_from_member(member: &str) -> StorageResult<&str> {
    if let Some(run_id) = member.strip_prefix("0:") {
        return Ok(run_id);
    }

    let bytes = member.as_bytes();
    if bytes.len() >= 19
        && bytes.starts_with(b"1:")
        && bytes[2..18].iter().all(u8::is_ascii_hexdigit)
        && bytes[18] == b':'
    {
        return Ok(&member[19..]);
    }

    Err(StorageError::corruption(
        "Invalid Redis ordered run catalog",
        "catalog member has an invalid format",
    ))
}

impl RedisStateStore {
    pub(super) async fn repair_ordered_catalog_entry(&self, run_id: &str) -> StorageResult<bool> {
        self.repair_ordered_catalog_entry_with_policy(run_id, false)
            .await
    }

    pub(super) async fn maintain_ordered_catalog_entry(&self, run_id: &str) -> StorageResult<()> {
        self.repair_ordered_catalog_entry_with_policy(run_id, true)
            .await
            .map(|_| ())
    }

    async fn repair_ordered_catalog_entry_with_policy(
        &self,
        run_id: &str,
        tolerate_snapshot_corruption: bool,
    ) -> StorageResult<bool> {
        let snapshot = match self.read_catalog_snapshot(run_id).await {
            Ok(snapshot) => snapshot,
            Err(error)
                if tolerate_snapshot_corruption && error.kind() == StorageErrorKind::Corruption =>
            {
                // Background maintenance must not make an unrelated newest
                // page deserialize a corrupt cold record. Traversing that
                // member through a user-visible page still reports it.
                return Ok(true);
            }
            Err(error) => return Err(error),
        };
        let Some(snapshot) = snapshot else {
            // A failed conditional sweep means a new incarnation now exists.
            // Initialization repairs that incarnation's catalog atomically, so
            // defer it instead of spinning against a concurrently changing run.
            return Ok(!self.remove_stale_index_entry(run_id).await?);
        };

        Ok(self
            .apply_ordered_catalog_snapshot_once(run_id, &snapshot)
            .await?
            .is_present())
    }

    async fn apply_ordered_catalog_snapshot_once(
        &self,
        run_id: &str,
        snapshot: &CatalogSnapshot,
    ) -> StorageResult<CatalogRepair> {
        match self.upsert_ordered_catalog_entry(run_id, snapshot).await? {
            CatalogUpsert::Updated => Ok(CatalogRepair::Repaired),
            CatalogUpsert::Missing => {
                if self.remove_stale_index_entry(run_id).await? {
                    Ok(CatalogRepair::Missing)
                } else {
                    // A delete/recreate race installed a newer incarnation and
                    // its initialization already repaired the catalog.
                    Ok(CatalogRepair::Deferred)
                }
            }
            // Every successful state CAS updates the catalog in the same Lua
            // script. A revision conflict therefore proves a newer catalog
            // entry exists; retry it during a later bounded maintenance cycle.
            CatalogUpsert::Conflict => Ok(CatalogRepair::Deferred),
        }
    }

    async fn read_ordered_batch(
        &self,
        key: &str,
        max: &str,
        count: usize,
    ) -> StorageResult<Vec<String>> {
        let mut conn = self.conn.clone();
        redis::cmd("ZREVRANGEBYLEX")
            .arg(key)
            .arg(max)
            .arg("-")
            .arg("LIMIT")
            .arg(0_u8)
            .arg(count)
            .query_async(&mut conn)
            .await
            .map_err(|error| map_redis_error("Failed to page Redis run catalog", error))
    }

    pub(super) async fn page_run_summaries(
        &self,
        query: &RunListQuery,
    ) -> StorageResult<RunSummaryPage> {
        loop {
            let generation = self.ensure_ordered_catalog().await?;
            self.maintain_ordered_catalog().await?;
            let page = self.read_run_summaries_page(query).await?;
            if self.catalog_generation_is_current(&generation).await? {
                return Ok(page);
            }
            tokio::task::yield_now().await;
        }
    }

    async fn read_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage> {
        let key = query
            .status()
            .map(|status| self.ordered_status_key(status))
            .unwrap_or_else(|| self.ordered_catalog_key());
        let mut max = query.after().map_or_else(
            || "+".to_string(),
            |cursor| format!("({}", ordered_member_parts(cursor.started(), cursor.id())),
        );
        let capacity = query
            .limit()
            .get()
            .checked_add(1)
            .expect("PageSize reserves room for a continuation");
        let mut items = Vec::with_capacity(capacity);
        let mut seen = HashSet::with_capacity(capacity);

        while items.len() < capacity {
            let members = self
                .read_ordered_batch(&key, &max, capacity - items.len())
                .await?;
            let Some(last_member) = members.last() else {
                break;
            };
            max = format!("({last_member}");

            for member in members {
                let run_id = run_id_from_member(&member)?;
                if seen.contains(run_id) {
                    continue;
                }

                let mut summary = self.read_summary(run_id).await?;
                if summary.as_ref().is_none_or(|summary| {
                    ordered_member(summary) != member
                        || query
                            .status()
                            .is_some_and(|status| summary.status != *status)
                }) {
                    if !self.repair_ordered_catalog_entry(run_id).await? {
                        continue;
                    }
                    summary = self.read_summary(run_id).await?;
                }

                let Some(summary) = summary else {
                    continue;
                };
                if ordered_member(&summary) != member
                    || query
                        .status()
                        .is_some_and(|status| summary.status != *status)
                {
                    continue;
                }
                seen.insert(summary.id.clone());
                items.push(summary);
                if items.len() == capacity {
                    break;
                }
            }
        }
        Ok(RunSummaryPage::from_ordered(items, query))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum CatalogRepair {
    Repaired,
    Missing,
    Deferred,
}

impl CatalogRepair {
    fn is_present(&self) -> bool {
        !matches!(self, Self::Missing)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::{TimeZone as _, Utc};
    use uuid::Uuid;

    use super::*;
    use crate::engine::types::RunStatus;
    use crate::storage::StateStore as _;

    fn summary(id: &str, micros: Option<i64>) -> RunSummary {
        RunSummary {
            id: id.to_string(),
            flow_name: "flow".to_string(),
            status: RunStatus::Pending,
            started: micros.map(|micros| Utc.timestamp_micros(micros).single().unwrap()),
            finished: None,
            task_count: 0,
        }
    }

    #[test]
    fn ordered_members_match_public_newest_first_order() {
        let mut values = [
            summary("same-a", Some(10)),
            summary("missing-z", None),
            summary("new", Some(11)),
            summary("same-z", Some(10)),
            summary("before-epoch", Some(-1)),
        ];
        values.sort_by_key(ordered_member);
        values.reverse();
        assert_eq!(
            values
                .iter()
                .map(|summary| summary.id.as_str())
                .collect::<Vec<_>>(),
            ["new", "same-z", "same-a", "before-epoch", "missing-z"]
        );
        for value in values {
            assert_eq!(
                run_id_from_member(&ordered_member(&value)).unwrap(),
                value.id
            );
        }
    }

    #[test]
    fn malformed_ordered_members_are_rejected() {
        for member in ["", "1:123:run", "2:0000000000000000:run"] {
            assert!(run_id_from_member(member).is_err());
        }
    }

    #[tokio::test]
    async fn stale_catalog_snapshot_is_deferred_after_one_attempt() {
        let Ok(url) = std::env::var("IRONFLOW_REDIS_TEST_URL") else {
            return;
        };
        let prefix = format!(
            "ironflow:test:catalog-repair-conflict:{}:",
            Uuid::new_v4().simple()
        );
        let store = RedisStateStore::new(&url, Some(prefix), None)
            .await
            .unwrap();
        let run_id = "hot-run";
        store
            .init_run(run_id, "flow", &crate::engine::types::Context::new())
            .await
            .unwrap();
        let stale = store.read_catalog_snapshot(run_id).await.unwrap().unwrap();
        store
            .set_run_status(run_id, RunStatus::Running)
            .await
            .unwrap();

        let repair = tokio::time::timeout(
            Duration::from_secs(1),
            store.apply_ordered_catalog_snapshot_once(run_id, &stale),
        )
        .await
        .expect("a catalog revision conflict retried instead of being deferred")
        .unwrap();
        assert_eq!(repair, CatalogRepair::Deferred);

        store.delete_run_atomic(run_id).await.unwrap();
        let mut conn = store.conn.clone();
        let _: usize = redis::cmd("DEL")
            .arg(store.ordered_catalog_ready_key())
            .query_async(&mut conn)
            .await
            .unwrap();
    }
}
