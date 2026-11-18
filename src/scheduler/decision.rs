//! The decision for one due instant: claim, skip or start.

use jiff::Timestamp;

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
        now: Timestamp,
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
        match self.post_claim_skip(name, schedule, instant, now).await {
            // Not a skip: the occurrence's own run exists and is recorded.
            Some(Outcome::NotClaimed) => return Outcome::NotClaimed,
            Some(skip) => return self.settle_skip(name, schedule, instant, skip).await,
            None => {}
        }
        match self.start(name, schedule, instant, claimed).await {
            skip @ (Outcome::AtCapacity | Outcome::Failed { .. }) => {
                self.settle_skip(name, schedule, instant, skip).await
            }
            outcome => outcome,
        }
    }

    /// Settle a missed instant that the more recent due instant `by`
    /// supersedes: claim it and record it as settled, so no replica starts
    /// it. One that already has its record (a run, or a skip recorded by a
    /// peer) is left as it is.
    pub(super) async fn supersede(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
        by: &str,
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
        tracing::debug!(
            schedule = %name,
            key = %instant.key,
            superseded_by = %by,
            "skipping missed instant: superseded by a more recent one"
        );
        let superseded = Outcome::Superseded { by: by.to_string() };
        self.settle_skip(name, schedule, instant, superseded).await
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

    /// Grace, overlap and capacity checks that consume the instant without a
    /// run, and the one check that finds its run already exists.
    async fn post_claim_skip(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
        now: Timestamp,
    ) -> Option<Outcome> {
        // Whole elapsed seconds, dropping the fraction: 300.9 seconds late
        // is 300, and a clock that reads earlier than the instant is negative.
        let lateness_seconds = now.duration_since(instant.instant.timestamp()).as_secs();
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
            if active_run == super::identity::run_id(name, &instant.key) {
                return Some(self.converge_on_own_run(name, schedule, instant).await);
            }
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

    /// The active run is this occurrence, not an earlier one it overlaps: an
    /// attempt that lost its result (a budget expiry, a peer that died before
    /// recording) created it. Converge on it as on any existing run.
    async fn converge_on_own_run(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
    ) -> Outcome {
        tracing::debug!(
            schedule = %name,
            key = %instant.key,
            "instant already has its run, which is still active"
        );
        self.record_run_created(name, schedule, instant).await;
        Outcome::NotClaimed
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
