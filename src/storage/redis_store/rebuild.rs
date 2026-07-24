use std::sync::LazyLock;
use std::time::Duration;

use uuid::Uuid;

use super::RedisStateStore;
use crate::storage::redis_config::map_redis_error;
use crate::storage::{StorageError, StorageResult};

const REBUILD_LEASE_MILLIS: u64 = 30_000;
const REBUILD_WAIT: Duration = Duration::from_millis(5);
const CATALOG_STATE_SCRIPT_SOURCE: &str = include_str!("scripts/catalog_state.lua");
const CATALOG_REBUILD_SCRIPT_SOURCE: &str = include_str!("scripts/catalog_rebuild.lua");

static CATALOG_STATE_SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(CATALOG_STATE_SCRIPT_SOURCE));
static CATALOG_REBUILD_SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(CATALOG_REBUILD_SCRIPT_SOURCE));

#[derive(Clone, Copy)]
enum RebuildAction {
    Acquire,
    Renew,
    Reset,
    Finalize,
    Release,
}

impl RebuildAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Acquire => "acquire",
            Self::Renew => "renew",
            Self::Reset => "reset",
            Self::Finalize => "finalize",
            Self::Release => "release",
        }
    }
}

struct CatalogState {
    generation: Option<String>,
    consistent: bool,
}

impl CatalogState {
    fn ready_generation(&self) -> Option<&str> {
        if self.consistent {
            self.generation.as_deref()
        } else {
            None
        }
    }
}

impl RedisStateStore {
    async fn catalog_state(&self) -> StorageResult<CatalogState> {
        let status_keys = self.ordered_status_keys();
        let mut conn = self.conn.clone();
        let (generation, consistent): (String, i64) = CATALOG_STATE_SCRIPT
            .key(self.ordered_catalog_ready_key())
            .key(self.index_key())
            .key(self.ordered_catalog_members_key())
            .key(self.ordered_catalog_key())
            .key(&status_keys[0])
            .key(&status_keys[1])
            .key(&status_keys[2])
            .key(&status_keys[3])
            .key(&status_keys[4])
            .key(&status_keys[5])
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error("Failed to inspect Redis ordered run catalog", error)
            })?;
        if !matches!(consistent, 0 | 1) {
            return Err(StorageError::corruption(
                "Invalid Redis ordered run catalog",
                "catalog state script returned an invalid consistency flag",
            ));
        }
        let generation = valid_generation(&generation).then_some(generation);
        Ok(CatalogState {
            consistent: consistent == 1 && generation.is_some(),
            generation,
        })
    }

    async fn rebuild_action(
        &self,
        action: RebuildAction,
        owner: &str,
        generation: Option<&str>,
    ) -> StorageResult<bool> {
        let status_keys = self.ordered_status_keys();
        let mut conn = self.conn.clone();
        let result: i64 = CATALOG_REBUILD_SCRIPT
            .key(self.ordered_catalog_rebuild_lock_key())
            .key(self.ordered_catalog_ready_key())
            .key(self.ordered_catalog_members_key())
            .key(self.ordered_catalog_key())
            .key(&status_keys[0])
            .key(&status_keys[1])
            .key(&status_keys[2])
            .key(&status_keys[3])
            .key(&status_keys[4])
            .key(&status_keys[5])
            .key(self.ordered_catalog_maintenance_cursor_key())
            .key(self.ordered_catalog_maintenance_high_water_key())
            .arg(action.as_str())
            .arg(owner)
            .arg(REBUILD_LEASE_MILLIS)
            .arg(generation.unwrap_or_default())
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to {} Redis run-catalog rebuild", action.as_str()),
                    error,
                )
            })?;
        match result {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(StorageError::corruption(
                "Invalid Redis run-catalog rebuild result",
                other,
            )),
        }
    }

    async fn rebuild_catalog(&self, owner: &str) -> StorageResult<Option<String>> {
        if let Some(generation) = self.catalog_state().await?.ready_generation() {
            let generation = generation.to_string();
            self.rebuild_action(RebuildAction::Release, owner, None)
                .await?;
            return Ok(Some(generation));
        }
        if !self
            .rebuild_action(RebuildAction::Reset, owner, None)
            .await?
        {
            return Ok(None);
        }

        let mut cursor = 0_u64;
        loop {
            if !self
                .rebuild_action(RebuildAction::Renew, owner, None)
                .await?
            {
                return Ok(None);
            }
            let (next, run_ids) = self.scan_index_batch(cursor).await?;
            for run_id in run_ids {
                if !self
                    .rebuild_action(RebuildAction::Renew, owner, None)
                    .await?
                {
                    return Ok(None);
                }
                self.repair_ordered_catalog_entry(&run_id).await?;
                // Repair is deliberately single-attempt, but its Redis round
                // trips can still outlive a stalled lease. Re-check ownership
                // before doing any more rebuild work or publishing readiness.
                if !self
                    .rebuild_action(RebuildAction::Renew, owner, None)
                    .await?
                {
                    return Ok(None);
                }
            }
            if next == 0 {
                break;
            }
            cursor = next;
        }

        let generation = Uuid::new_v4().simple().to_string();
        let finalized = self
            .rebuild_action(RebuildAction::Finalize, owner, Some(&generation))
            .await?;
        Ok(finalized.then_some(generation))
    }

    pub(super) async fn ensure_ordered_catalog(&self) -> StorageResult<String> {
        loop {
            if let Some(generation) = self.catalog_state().await?.ready_generation() {
                return Ok(generation.to_string());
            }

            let owner = Uuid::new_v4().simple().to_string();
            if !self
                .rebuild_action(RebuildAction::Acquire, &owner, None)
                .await?
            {
                tokio::time::sleep(REBUILD_WAIT).await;
                continue;
            }

            match self.rebuild_catalog(&owner).await {
                Ok(Some(generation)) => return Ok(generation),
                Ok(None) => tokio::task::yield_now().await,
                Err(error) => {
                    let _ = self
                        .rebuild_action(RebuildAction::Release, &owner, None)
                        .await;
                    return Err(error);
                }
            }
        }
    }

    pub(super) async fn catalog_generation_is_current(
        &self,
        expected: &str,
    ) -> StorageResult<bool> {
        let mut conn = self.conn.clone();
        let current: Option<String> = redis::cmd("GET")
            .arg(self.ordered_catalog_ready_key())
            .query_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error("Failed to verify Redis run-catalog generation", error)
            })?;
        Ok(current.as_deref() == Some(expected))
    }
}

fn valid_generation(value: &str) -> bool {
    value == "1"
        || Uuid::parse_str(value).is_ok_and(|generation| generation.simple().to_string() == value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn expired_rebuild_owner_cannot_renew_or_finalize() {
        let Ok(url) = std::env::var("IRONFLOW_REDIS_TEST_URL") else {
            return;
        };
        let prefix = format!(
            "ironflow:test:catalog-rebuild-lease:{}:",
            Uuid::new_v4().simple()
        );
        let store = RedisStateStore::new(&url, Some(prefix), None)
            .await
            .unwrap();
        let expired_owner = Uuid::new_v4().simple().to_string();
        let current_owner = Uuid::new_v4().simple().to_string();

        assert!(
            store
                .rebuild_action(RebuildAction::Acquire, &expired_owner, None)
                .await
                .unwrap()
        );
        assert!(
            store
                .rebuild_action(RebuildAction::Reset, &expired_owner, None)
                .await
                .unwrap()
        );
        let mut conn = store.conn.clone();
        let _: bool = redis::cmd("PEXPIRE")
            .arg(store.ordered_catalog_rebuild_lock_key())
            .arg(1_u8)
            .query_async(&mut conn)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            store
                .rebuild_action(RebuildAction::Acquire, &current_owner, None)
                .await
                .unwrap()
        );

        assert!(
            !store
                .rebuild_action(RebuildAction::Renew, &expired_owner, None)
                .await
                .unwrap()
        );
        let generation = Uuid::new_v4().simple().to_string();
        assert!(
            !store
                .rebuild_action(RebuildAction::Finalize, &expired_owner, Some(&generation))
                .await
                .unwrap()
        );
        let ready: Option<String> = redis::cmd("GET")
            .arg(store.ordered_catalog_ready_key())
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(ready.is_none());
        assert!(
            store
                .rebuild_action(RebuildAction::Release, &current_owner, None)
                .await
                .unwrap()
        );
    }
}
