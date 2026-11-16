//! Concurrent, time-bounded evaluation of configured schedule names.

use std::sync::{Mutex, PoisonError};

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
    /// The budget expired and what was undecided stays due.
    budget_deferred: bool,
}

/// What one evaluation has decided so far. It lives outside the future the
/// budget can cancel, so an expiry keeps every decision already made.
#[derive(Default)]
struct Progress {
    superseded: Vec<Decision>,
    current: Vec<Decision>,
    /// The loop over `DueWindow::current` ran to its end.
    current_done: bool,
    earliest_retry: Option<NaiveDateTime>,
}

type SharedProgress = Mutex<Progress>;

fn with<T>(progress: &SharedProgress, update: impl FnOnce(&mut Progress) -> T) -> T {
    // Never held across an await, and no update can panic part-way.
    update(&mut progress.lock().unwrap_or_else(PoisonError::into_inner))
}

const BUDGET_DEFERRED: &str = "schedule evaluation exceeded its budget before the run started";

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
            if evaluation.budget_deferred {
                *self
                    .budget_expiries
                    .entry(evaluation.name.clone())
                    .or_default() += 1;
            } else {
                self.budget_expiries.remove(&evaluation.name);
            }
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
        let grace_floor = timing::grace_floor(now, schedule);
        let progress = SharedProgress::default();

        let expired = tokio::time::timeout(
            self.evaluation_timeout,
            self.evaluate_window(name, schedule, &window, now, &progress),
        )
        .await
        .is_err();
        let mut progress = progress
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);

        let mut budget_deferred = false;
        let mut retry = earliest(progress.earliest_retry, window.resume_at);
        if expired {
            let undecided_superseded = &window.superseded[progress.superseded.len()..];
            let undecided_current: &[DueInstant] = if progress.current_done {
                &[]
            } else {
                &window.current[progress.current.len()..]
            };
            budget_deferred =
                self.budget_expiries.get(name).copied().unwrap_or(0) < super::MAX_BUDGET_DEFERRALS;
            log_expiry(
                name,
                self.evaluation_timeout,
                undecided_superseded.len() + undecided_current.len(),
                budget_deferred,
            );
            if budget_deferred {
                // What was not decided stays due. A missed instant is
                // superseded on a later tick and gets no decision now.
                let first = undecided_superseded.iter().chain(undecided_current).next();
                retry = earliest(retry, first.map(|instant| instant.local));
                let reason = || Outcome::Deferred {
                    reason: BUDGET_DEFERRED.to_string(),
                };
                progress.current.extend(
                    undecided_current
                        .iter()
                        .map(|instant| decision(name, instant, reason())),
                );
            } else {
                // The bound is reached: burn the window, as every expiry did
                // before IF-217.
                retry = None;
                let burned = |instant| decision(name, instant, Outcome::TimedOut);
                progress
                    .superseded
                    .extend(undecided_superseded.iter().map(burned));
                progress
                    .current
                    .extend(undecided_current.iter().map(burned));
            }
        }

        let mut decisions = progress.superseded;
        decisions.extend(progress.current);
        Evaluation {
            name: name.to_string(),
            target: timing::watermark_target(window.through, grace_floor, retry),
            decisions,
            budget_deferred,
        }
    }

    /// Decide the most recent due occurrence, then settle the missed ones it
    /// supersedes (see `window`). The most recent goes first because it is
    /// the only one that can start a run; the decisions are still reported
    /// oldest first. Every result goes to `progress` as soon as it exists,
    /// because the caller's budget can cancel this future at any await.
    async fn evaluate_window(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        window: &DueWindow,
        now: DateTime<Utc>,
        progress: &SharedProgress,
    ) {
        for instant in &window.current {
            let outcome = self.decide(name, schedule, instant, now).await;
            let deferred = matches!(outcome, Outcome::Deferred { .. });
            let retried = deferred || matches!(outcome, Outcome::ClaimFailed { .. });
            with(progress, |progress| {
                if retried {
                    progress.earliest_retry.get_or_insert(instant.local);
                }
                progress.current.push(decision(name, instant, outcome));
            });
            // The watermark stops before a deferred instant; waiting for the
            // load limit again for a matched time that shares its key would
            // only spend this schedule's evaluation budget.
            if deferred {
                break;
            }
        }
        with(progress, |progress| progress.current_done = true);

        let Some(by) = window.current.first() else {
            return;
        };
        for instant in &window.superseded {
            let outcome = self.supersede(name, schedule, instant, &by.key).await;
            with(progress, |progress| {
                if matches!(outcome, Outcome::ClaimFailed { .. }) {
                    progress.earliest_retry =
                        earliest(progress.earliest_retry, Some(instant.local));
                }
                progress.superseded.push(decision(name, instant, outcome));
            });
        }
        with(progress, |progress| {
            log_superseded(
                name,
                &progress.superseded,
                &by.key,
                window.resume_at.is_some(),
            );
        });
    }
}

fn log_expiry(name: &str, timeout: std::time::Duration, undecided: usize, deferred: bool) {
    if deferred {
        tracing::warn!(
            schedule = %name,
            timeout_ms = timeout.as_millis(),
            undecided_instants = undecided,
            "schedule evaluation exceeded its budget; undecided instants stay due \
             and are retried on the next tick"
        );
    } else {
        tracing::error!(
            schedule = %name,
            timeout_ms = timeout.as_millis(),
            undecided_instants = undecided,
            consecutive_expiries = super::MAX_BUDGET_DEFERRALS + 1,
            "schedule evaluation timed out again; undecided instants are \
             burned rather than retried"
        );
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
