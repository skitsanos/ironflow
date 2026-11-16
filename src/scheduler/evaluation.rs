//! Concurrent, time-bounded evaluation of configured schedule names.

use chrono::{DateTime, NaiveDateTime, Utc};
use futures_util::future::join_all;

use super::config::ScheduleConfig;
use super::timing::{self, DueInstant};
use super::window::{DueWindow, due_window};
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
        let window = due_window(schedule, self.evaluated_through[name], through);

        match tokio::time::timeout(
            self.evaluation_timeout,
            self.evaluate_window(name, schedule, &window, now),
        )
        .await
        {
            Ok((decisions, earliest_retry)) => {
                let grace_floor = timing::grace_floor(now, schedule);
                let retry = earliest(earliest_retry, window.resume_at);
                Evaluation {
                    name: name.to_string(),
                    target: timing::watermark_target(window.through, grace_floor, retry),
                    decisions,
                }
            }
            Err(_) => {
                let due = window.superseded.iter().chain(&window.current);
                let decisions = due
                    .map(|instant| decision(name, instant, Outcome::TimedOut))
                    .collect::<Vec<_>>();
                tracing::error!(
                    schedule = %name,
                    timeout_ms = self.evaluation_timeout.as_millis(),
                    due_instants = decisions.len(),
                    "schedule evaluation timed out; due instants are indeterminate \
                     and will be burned rather than retried"
                );
                Evaluation {
                    name: name.to_string(),
                    target: window.through,
                    decisions,
                }
            }
        }
    }

    /// Decide the most recent due occurrence, then settle the missed ones it
    /// supersedes (see `window`). The most recent goes first because it is
    /// the only one that can start a run; the decisions are still returned
    /// oldest first. The second value is the earliest instant to retry.
    async fn evaluate_window(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        window: &DueWindow,
        now: DateTime<Utc>,
    ) -> (Vec<Decision>, Option<NaiveDateTime>) {
        let mut earliest_retry = None;
        let mut current = Vec::with_capacity(window.current.len());
        for instant in &window.current {
            let outcome = self.decide(name, schedule, instant, now).await;
            let deferred = matches!(outcome, Outcome::Deferred { .. });
            if deferred || matches!(outcome, Outcome::ClaimFailed { .. }) {
                earliest_retry.get_or_insert(instant.local);
            }
            current.push(decision(name, instant, outcome));
            // The watermark stops before a deferred instant; waiting for the
            // load limit again for a matched time that shares its key would
            // only spend this schedule's evaluation budget.
            if deferred {
                break;
            }
        }

        let mut decisions = Vec::with_capacity(window.superseded.len() + current.len());
        if let Some(by) = window.current.first() {
            for instant in &window.superseded {
                let outcome = self.supersede(name, schedule, instant, &by.key).await;
                if matches!(outcome, Outcome::ClaimFailed { .. }) {
                    earliest_retry = earliest(earliest_retry, Some(instant.local));
                }
                decisions.push(decision(name, instant, outcome));
            }
            log_superseded(name, &decisions, &by.key, window.resume_at.is_some());
        }
        decisions.extend(current);
        (decisions, earliest_retry)
    }
}

fn decision(name: &str, instant: &DueInstant, outcome: Outcome) -> Decision {
    Decision {
        schedule: name.to_string(),
        key: instant.key.clone(),
        outcome,
    }
}

fn earliest(a: Option<NaiveDateTime>, b: Option<NaiveDateTime>) -> Option<NaiveDateTime> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// One line per tick for the instants a catch-up superseded, however many:
/// a week of missed minutes must not become ten thousand warnings.
fn log_superseded(name: &str, decisions: &[Decision], by: &str, more: bool) {
    let mut superseded = decisions
        .iter()
        .filter(|decision| matches!(decision.outcome, Outcome::Superseded { .. }));
    let Some(first) = superseded.next() else {
        return;
    };
    let last = superseded.next_back().unwrap_or(first);
    tracing::warn!(
        schedule = %name,
        superseded = 2 + superseded.count(),
        from = %first.key,
        to = %last.key,
        superseded_by = %by,
        more_on_later_ticks = more,
        "skipping missed instants: at catch-up only the most recent one is decided"
    );
}
