//! The decision for one due instant: claim, skip or start.

use chrono::{DateTime, Utc};

use super::config::ScheduleConfig;
use super::timing::DueInstant;
use super::{Outcome, Scheduler};

impl Scheduler {
    /// Claim first so replicas cannot reach different post-claim decisions and
    /// both run. Every post-claim skip deliberately consumes the instant, and
    /// records that durably, so a replica that restarts inside the grace
    /// window finds it settled instead of recovering it as an orphaned claim
    /// (IF-217). A start deferred for flow-load capacity consumes nothing.
    ///
    /// A lost claim whose occurrence already produced a run is never started
    /// again, even when that run's record has expired or been deleted
    /// (IF-201), and neither is one that was skipped. A lost claim without
    /// an occurrence record is an orphan and proceeds.
    pub(super) async fn decide(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
        now: DateTime<Utc>,
    ) -> Outcome {
        let claimed = match self.claim(name, schedule, instant).await {
            Ok(claimed) => claimed,
            Err(outcome) => return outcome,
        };
        if !claimed {
            match self.run_already_created(name, instant).await {
                Ok(true) => return Outcome::NotClaimed,
                Ok(false) => {}
                Err(outcome) => return outcome,
            }
        }
        if let Some(skip) = self.post_claim_skip(name, schedule, instant, now).await {
            return self.settle_skip(name, schedule, instant, skip).await;
        }
        match self.start(name, schedule, instant, claimed).await {
            skip @ (Outcome::AtCapacity | Outcome::Failed { .. }) => {
                self.settle_skip(name, schedule, instant, skip).await
            }
            outcome => outcome,
        }
    }

    /// Own the instant durably. `Ok(false)` means a peer claimed it first;
    /// `decide` then consults the occurrence record before this replica
    /// converges on the peer's deterministic run identity.
    async fn claim(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
    ) -> Result<bool, Outcome> {
        match self
            .store
            .claim_schedule(name, &instant.key, schedule.claim_ttl_seconds())
            .await
        {
            Ok(true) => Ok(true),
            Ok(false) => {
                tracing::debug!(
                    schedule = %name,
                    key = %instant.key,
                    "instant claimed by a peer; converging on its deterministic run identity"
                );
                Ok(false)
            }
            Err(error) => {
                tracing::warn!(schedule = %name, key = %instant.key, %error, "claim failed");
                Err(Outcome::ClaimFailed {
                    error: error.to_string(),
                })
            }
        }
    }

    /// Grace, overlap and capacity checks that consume the instant without a run.
    async fn post_claim_skip(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
        now: DateTime<Utc>,
    ) -> Option<Outcome> {
        let lateness_seconds = (now - instant.instant.with_timezone(&Utc)).num_seconds();
        if lateness_seconds > schedule.grace_seconds_i64() {
            tracing::warn!(
                schedule = %name,
                key = %instant.key,
                lateness_seconds,
                grace_seconds = schedule.grace_seconds(),
                "skipping missed instant: past its grace window"
            );
            return Some(Outcome::Late { lateness_seconds });
        }

        if let Some(active_run) = self.executor.active_run(name).await {
            tracing::warn!(
                schedule = %name,
                key = %instant.key,
                %active_run,
                "skipping instant: a previous run has not finished"
            );
            return Some(Outcome::Overlapped { active_run });
        }

        if !self.executor.has_capacity() {
            return Some(at_capacity(name, &instant.key));
        }

        None
    }

    async fn start(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
        claimed: bool,
    ) -> Outcome {
        match self.run_and_record(name, schedule, instant).await {
            Ok(super::ScheduleRun::Started { run_id }) => {
                tracing::info!(schedule = %name, key = %instant.key, %run_id, "scheduled run started");
                Outcome::Fired { run_id }
            }
            Ok(super::ScheduleRun::Existing { run_id }) => {
                tracing::debug!(
                    schedule = %name,
                    key = %instant.key,
                    %run_id,
                    claimed,
                    "schedule occurrence already has a durable run"
                );
                Outcome::NotClaimed
            }
            Ok(super::ScheduleRun::Deferred { reason }) => deferred(name, &instant.key, reason),
            // The cap filled after the probe: the same skip, not a failure.
            Ok(super::ScheduleRun::AtCapacity) => at_capacity(name, &instant.key),
            Err(error) => {
                tracing::warn!(schedule = %name, key = %instant.key, %error, "scheduled run failed");
                Outcome::Failed { error }
            }
        }
    }
}

fn deferred(name: &str, key: &str, reason: String) -> Outcome {
    tracing::warn!(
        schedule = %name,
        %key,
        %reason,
        "scheduled run deferred; it is retried on the next tick within grace"
    );
    Outcome::Deferred { reason }
}

fn at_capacity(name: &str, key: &str) -> Outcome {
    tracing::warn!(
        schedule = %name,
        %key,
        "skipping instant: at maximum concurrent run capacity"
    );
    Outcome::AtCapacity
}
