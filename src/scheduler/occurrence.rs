//! The durable "run created" record of one schedule occurrence (IF-201).
//!
//! The deterministic run ID fences an occurrence only while its run record
//! exists. Once the record is gone (a Redis TTL, `DELETE /runs/{id}`), a lost
//! claim alone cannot tell "a peer finished this instant" from "a peer died
//! before starting it". The claim therefore records that the run was created,
//! and a replica that loses the claim consults that record before it starts
//! anything.

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

    /// Record that the occurrence has a durable run. Called for a run this
    /// replica started and for one it converged on, so a peer repairs the
    /// record of a replica that died between creating the run and writing it.
    ///
    /// Best-effort by design: the run exists and remains the fence for as
    /// long as its record does, so a failed write must not fail the instant.
    async fn record_run_created(
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
