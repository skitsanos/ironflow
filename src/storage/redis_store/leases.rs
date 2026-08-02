use std::sync::LazyLock;

use chrono::{DateTime, Utc};

use super::RedisStateStore;
use super::lease_cas::LeaseGuard;
use crate::engine::types::{Context, RunStatus, TaskState, TaskStatus};
use crate::storage::redis_config::map_redis_error;
use crate::storage::{RunLease, StorageError, StorageResult};

const RENEW_SOURCE: &str = include_str!("scripts/lease_renew.lua");
const RECONCILE_BATCH: usize = 256;

static RENEW_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| redis::Script::new(RENEW_SOURCE));
impl RedisStateStore {
    pub(super) async fn renew_owned_run(
        &self,
        run_id: &str,
        lease: &RunLease,
    ) -> StorageResult<bool> {
        let run_key = self.resolve_run_key(run_id).await?;
        let mut conn = self.conn.clone();
        let renewed: i64 = RENEW_SCRIPT
            .key(run_key)
            .key(self.run_lease_expiry_key())
            .arg(lease.owner())
            .arg(run_id)
            .arg(crate::storage::RUN_LEASE_TTL.as_micros())
            .arg(self.ttl.unwrap_or(-1))
            .arg(crate::storage::run_lease::RUN_LEASE_KEY_SAFETY.as_micros())
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to renew Redis run lease '{run_id}'"),
                    error,
                )
            })?;
        match renewed {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(StorageError::corruption(
                format_args!("Invalid Redis run lease result for '{run_id}'"),
                value,
            )),
        }
    }

    pub(super) async fn set_owned_status(
        &self,
        run_id: &str,
        status: RunStatus,
        owner: &str,
    ) -> StorageResult<bool> {
        Ok(self
            .guarded_mutation(run_id, LeaseGuard::Owner(owner), move |info| {
                info.status = status.clone();
                if status.is_terminal() && info.finished.is_none() {
                    info.finished = Some(Utc::now());
                }
            })
            .await?
            .is_some())
    }

    pub(super) async fn upsert_owned_task(
        &self,
        run_id: &str,
        task: &TaskState,
        owner: &str,
    ) -> StorageResult<bool> {
        Ok(self
            .guarded_mutation(run_id, LeaseGuard::Owner(owner), |info| {
                info.tasks.insert(task.name.clone(), task.clone());
            })
            .await?
            .is_some())
    }

    pub(super) async fn update_owned_context(
        &self,
        run_id: &str,
        ctx: &Context,
        owner: &str,
    ) -> StorageResult<bool> {
        Ok(self
            .guarded_mutation(run_id, LeaseGuard::Owner(owner), |info| {
                info.ctx.extend(ctx.clone());
            })
            .await?
            .is_some())
    }

    pub(super) async fn reconcile_owned_runs(&self, _now: DateTime<Utc>) -> StorageResult<usize> {
        let mut reconciled = 0;
        loop {
            let mut conn = self.conn.clone();
            let (seconds, micros): (i64, i64) = redis::cmd("TIME")
                .query_async(&mut conn)
                .await
                .map_err(|error| map_redis_error("Failed to read Redis server time", error))?;
            let cutoff = seconds.saturating_mul(1_000_000).saturating_add(micros);
            let candidates: Vec<String> = redis::cmd("ZRANGEBYSCORE")
                .arg(self.run_lease_expiry_key())
                .arg("-inf")
                .arg(cutoff)
                .arg("LIMIT")
                .arg(0_u8)
                .arg(RECONCILE_BATCH)
                .query_async(&mut conn)
                .await
                .map_err(|error| {
                    map_redis_error("Failed to list expired Redis run leases", error)
                })?;
            if candidates.is_empty() {
                return Ok(reconciled);
            }
            for run_id in candidates {
                match self
                    .guarded_mutation(&run_id, LeaseGuard::Expired, |info| {
                        let was_nonterminal = !info.status.is_terminal();
                        if was_nonterminal {
                            let finished = Utc::now();
                            for task in info.tasks.values_mut() {
                                if !task.status.is_terminal() {
                                    task.status = if task.status == TaskStatus::Running {
                                        TaskStatus::Failed
                                    } else {
                                        TaskStatus::Skipped
                                    };
                                    task.error = Some(
                                        "task stopped after execution-owner lease expired"
                                            .to_string(),
                                    );
                                    task.finished = Some(finished);
                                }
                            }
                            info.status = RunStatus::Stalled;
                            info.finished.get_or_insert(finished);
                        }
                        was_nonterminal
                    })
                    .await
                {
                    Ok(Some(stalled)) => reconciled += usize::from(stalled),
                    Ok(None) => {}
                    Err(error) if error.is_not_found() => {
                        self.remove_stale_index_entry(&run_id).await?;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    }
}
