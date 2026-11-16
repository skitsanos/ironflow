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
            Ok((decisions, earliest_retry)) => {
                let grace_floor = timing::grace_floor(now, schedule);
                let target = timing::watermark_target(through, grace_floor, earliest_retry);
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
        let mut earliest_retry = None;

        for instant in due {
            let outcome = self.decide(name, schedule, &instant, now).await;
            let deferred = matches!(outcome, Outcome::Deferred { .. });
            if deferred || matches!(outcome, Outcome::ClaimFailed { .. }) {
                earliest_retry.get_or_insert(instant.local);
            }
            decisions.push(Decision {
                schedule: name.to_string(),
                key: instant.key,
                outcome,
            });
            // The watermark stops before a deferred instant, so the later
            // ones come back next tick in order; waiting for the load limit
            // again here would only spend this schedule's evaluation budget.
            if deferred {
                break;
            }
        }

        (decisions, earliest_retry)
    }
}
