//! Concurrent, time-bounded evaluation of configured schedule names.

use chrono::{DateTime, NaiveDateTime, Utc};
use futures_util::future::join_all;

use super::config::ScheduleConfig;
use super::timing::{self, DueInstant, due_instants};
use super::{Decision, Outcome, Scheduler};

struct Evaluation {
    name: String,
    target: NaiveDateTime,
    decisions: Vec<Decision>,
}

impl Scheduler {
    /// Evaluate every schedule once and return deterministic, name-sorted
    /// decisions. Names execute concurrently so one stalled backend call does
    /// not hold unrelated schedules behind it.
    pub async fn evaluate(&mut self, now: DateTime<Utc>) -> Vec<Decision> {
        let mut names: Vec<String> = self.schedules.keys().cloned().collect();
        names.sort();

        let evaluations = names.iter().map(|name| self.evaluate_name(name, now));
        let completed = join_all(evaluations).await;
        let mut decisions = Vec::new();

        for evaluation in completed {
            let after = self.evaluated_through[&evaluation.name];
            if evaluation.target > after {
                self.evaluated_through
                    .insert(evaluation.name, evaluation.target);
            }
            decisions.extend(evaluation.decisions);
        }

        if let Some(metrics) = &self.metrics {
            for decision in &decisions {
                metrics.scheduler(crate::metrics::SchedulerOutcome::from_outcome(
                    &decision.outcome,
                ));
            }
        }

        decisions
    }

    async fn evaluate_name(&self, name: &str, now: DateTime<Utc>) -> Evaluation {
        let schedule = &self.schedules[name];
        let through = now.with_timezone(&schedule.timezone()).naive_local();
        let after = self.evaluated_through[name];
        let (due, truncated) = due_instants(schedule, after, through);

        if truncated {
            tracing::warn!(
                schedule = %name,
                limit = timing::MAX_INSTANTS_PER_TICK,
                "schedule produced more due instants than one tick evaluates; \
                 the remainder are skipped, not queued"
            );
        }

        let timeout_keys = due
            .iter()
            .map(|instant| instant.key.clone())
            .collect::<Vec<_>>();
        match tokio::time::timeout(
            self.evaluation_timeout,
            self.evaluate_due(name, schedule, due, now),
        )
        .await
        {
            Ok((decisions, earliest_claim_error)) => {
                let grace_floor = timing::grace_floor(now, schedule);
                let target = timing::watermark_target(through, grace_floor, earliest_claim_error);
                Evaluation {
                    name: name.to_string(),
                    target,
                    decisions,
                }
            }
            Err(_) => {
                tracing::error!(
                    schedule = %name,
                    timeout_ms = self.evaluation_timeout.as_millis(),
                    due_instants = timeout_keys.len(),
                    "schedule evaluation timed out; due instants are indeterminate \
                     and will be burned rather than retried"
                );
                Evaluation {
                    name: name.to_string(),
                    target: through,
                    decisions: timeout_keys
                        .into_iter()
                        .map(|key| Decision {
                            schedule: name.to_string(),
                            key,
                            outcome: Outcome::TimedOut,
                        })
                        .collect(),
                }
            }
        }
    }

    async fn evaluate_due(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        due: Vec<DueInstant>,
        now: DateTime<Utc>,
    ) -> (Vec<Decision>, Option<NaiveDateTime>) {
        let mut decisions = Vec::with_capacity(due.len());
        let mut earliest_claim_error = None;

        for instant in due {
            let outcome = self.decide(name, schedule, &instant, now).await;
            if matches!(outcome, Outcome::ClaimFailed { .. }) {
                earliest_claim_error.get_or_insert(instant.local);
            }
            decisions.push(Decision {
                schedule: name.to_string(),
                key: instant.key,
                outcome,
            });
        }

        (decisions, earliest_claim_error)
    }

    /// Claim first so replicas cannot reach different post-claim decisions and
    /// both run. Every post-claim skip deliberately consumes the instant.
    async fn decide(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &DueInstant,
        now: DateTime<Utc>,
    ) -> Outcome {
        let claimed = match self
            .store
            .claim_schedule(name, &instant.key, schedule.claim_ttl_seconds())
            .await
        {
            Ok(true) => true,
            Ok(false) => {
                tracing::debug!(
                    schedule = %name,
                    key = %instant.key,
                    "instant claimed by a peer; converging on its deterministic run identity"
                );
                false
            }
            Err(error) => {
                tracing::warn!(schedule = %name, key = %instant.key, %error, "claim failed");
                return Outcome::ClaimFailed {
                    error: error.to_string(),
                };
            }
        };

        let lateness_seconds = (now - instant.instant.with_timezone(&Utc)).num_seconds();
        if lateness_seconds > schedule.grace_seconds_i64() {
            tracing::warn!(
                schedule = %name,
                key = %instant.key,
                lateness_seconds,
                grace_seconds = schedule.grace_seconds(),
                "skipping missed instant: past its grace window"
            );
            return Outcome::Late { lateness_seconds };
        }

        if let Some(active_run) = self.executor.active_run(name).await {
            tracing::warn!(
                schedule = %name,
                key = %instant.key,
                %active_run,
                "skipping instant: a previous run has not finished"
            );
            return Outcome::Overlapped { active_run };
        }

        if !self.executor.has_capacity() {
            tracing::warn!(
                schedule = %name,
                key = %instant.key,
                "skipping instant: at maximum concurrent run capacity"
            );
            return Outcome::AtCapacity;
        }

        match self.executor.run(name, &instant.key, schedule).await {
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
            Err(error) => {
                tracing::warn!(schedule = %name, key = %instant.key, %error, "scheduled run failed");
                Outcome::Failed { error }
            }
        }
    }
}
