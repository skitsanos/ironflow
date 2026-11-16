//! The durable record that one schedule occurrence is settled (IF-201).
//!
//! The deterministic run ID fences an occurrence only while its run record
//! exists. Once the record is gone (a Redis TTL, `DELETE /runs/{id}`), a lost
//! claim alone cannot tell "a peer finished this instant" from "a peer died
//! before starting it". The claim therefore records that the run was created,
//! and a replica that loses the claim consults that record before it starts
//! anything.
//!
//! The same record settles an occurrence the scheduler decided not to start
//! (IF-217). A skip leaves no run behind, so without the record its claim
//! reads as an orphan and a replica that restarts inside the grace window
//! starts the instant its peer had skipped. The stores keep one mark per
//! claim, written by `record_schedule_run`: it means "do not start this
//! occurrence", whether or not a run exists.

use super::config::ScheduleConfig;
use super::timing::DueInstant;
use super::{Outcome, ScheduleRun, Scheduler};

impl Scheduler {
    /// Whether a lost claim belongs to an occurrence whose run was already
    /// created. `Ok(false)` is an orphaned claim that IF-078 still recovers.
    ///
    /// A read failure is not evidence either way, and starting on it could run
    /// the flow twice, so the instant is retried like a failed claim.
    pub(super) async fn run_already_created(
        &self,
        name: &str,
        instant: &DueInstant,
    ) -> Result<bool, Outcome> {
        match self.store.schedule_run_recorded(name, &instant.key).await {
            Ok(true) => {
                tracing::debug!(
                    schedule = %name,
                    key = %instant.key,
                    "instant already produced a run; not starting it again"
                );
                Ok(true)
            }
            Ok(false) => Ok(false),
            Err(error) => {
                tracing::warn!(
                    schedule = %name,
                    key = %instant.key,
                    %error,
                    "could not read the occurrence record of a claimed instant"
                );
                Err(Outcome::ClaimFailed {
                    error: error.to_string(),
                })
            }
        }
    }

    /// Hand the occurrence to the executor and record every result that
    /// leaves a durable run behind.
    pub(super) async fn run_and_record(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
    ) -> Result<ScheduleRun, String> {
        let result = self.executor.run(name, &instant.key, schedule).await;
        if matches!(
            result,
            Ok(ScheduleRun::Started { .. } | ScheduleRun::Existing { .. })
        ) {
            self.record_run_created(name, schedule, instant).await;
        }
        result
    }

    /// Make a skip durable: record the occurrence as settled although it has
    /// no run, and return `skip` to report.
    ///
    /// Unlike the record of a started run, nothing else fences a skipped
    /// occurrence, so a failed write is not ignored. The instant is reported
    /// as a failed claim and retried: the next tick decides it again and
    /// either records the skip or, if the reason has gone, starts it. That is
    /// the decision an unrecorded skip would leave to any restarted replica.
    pub(super) async fn settle_skip(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
        skip: Outcome,
    ) -> Outcome {
        match self
            .store
            .record_schedule_run(name, &instant.key, schedule.claim_ttl_seconds())
            .await
        {
            Ok(()) => skip,
            Err(error) => {
                tracing::warn!(
                    schedule = %name,
                    key = %instant.key,
                    %error,
                    "could not record a skipped instant as settled; it is retried"
                );
                Outcome::ClaimFailed {
                    error: error.to_string(),
                }
            }
        }
    }

    /// Record that the occurrence has a durable run. Called for a run this
    /// replica started and for one it converged on, so a peer repairs the
    /// record of a replica that died between creating the run and writing it.
    ///
    /// Best-effort by design: the run exists and remains the fence for as
    /// long as its record does, so a failed write must not fail the instant.
    pub(super) async fn record_run_created(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
    ) {
        if let Err(error) = self
            .store
            .record_schedule_run(name, &instant.key, schedule.claim_ttl_seconds())
            .await
        {
            tracing::warn!(
                schedule = %name,
                key = %instant.key,
                %error,
                "could not record that the occurrence has a run; it stays fenced \
                 only while its run record exists"
            );
        }
    }
}
