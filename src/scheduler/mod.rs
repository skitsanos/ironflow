//! Cron-driven flow triggers for `ironflow serve`.

pub mod config;
pub mod execution;
pub mod timing;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};

use crate::storage::StateStore;

use config::ScheduleConfig;
use timing::due_instants;

/// How often the scheduler evaluates every schedule.
///
/// Cron's finest granularity here is one minute, so a 30-second tick cannot
/// miss a minute boundary.
pub const TICK_INTERVAL: Duration = Duration::from_secs(30);

/// What happened to one due instant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The instant was owned and executed.
    Fired { run_id: String },
    /// Another replica owns this instant.
    NotClaimed,
    /// Past the grace window.
    Late { lateness_seconds: i64 },
    /// A previous run of this schedule has not finished.
    Overlapped { active_run: String },
    /// The process is at `IRONFLOW_MAX_CONCURRENT_RUNS`.
    AtCapacity,
    /// The flow failed to load or execute. This run failed; the scheduler did
    /// not.
    Failed { error: String },
}

/// One evaluated instant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub schedule: String,
    pub key: String,
    pub outcome: Outcome,
}

/// What the tick does once an instant is owned.
///
/// A trait so the decision order can be tested without executing a workflow.
#[async_trait]
pub trait ScheduleExecutor: Send + Sync {
    /// Run id of a non-terminal run belonging to `schedule_name`, if any.
    async fn active_run(&self, schedule_name: &str) -> Option<String>;

    /// Whether the process is below its concurrent-run cap.
    fn has_capacity(&self) -> bool;

    /// Execute the schedule's flow, returning the new run id.
    async fn run(&self, schedule_name: &str, schedule: &ScheduleConfig) -> Result<String, String>;
}

/// Evaluates every schedule on a tick.
pub struct Scheduler {
    schedules: HashMap<String, ScheduleConfig>,
    store: Arc<dyn StateStore>,
    executor: Arc<dyn ScheduleExecutor>,
    /// Local wall-clock time each schedule was last evaluated through.
    evaluated_through: HashMap<String, NaiveDateTime>,
}

impl Scheduler {
    /// Seeds each schedule's watermark at `now - grace_seconds`, so a process
    /// that starts after an outage catches up only as far as an instant could
    /// still legitimately fire, rather than replaying history. The subtraction
    /// happens in UTC, before converting to local time: grace measures real
    /// elapsed time, not wall-clock, and the two only agree when the UTC
    /// offset is constant across the grace window.
    pub fn new(
        schedules: HashMap<String, ScheduleConfig>,
        store: Arc<dyn StateStore>,
        executor: Arc<dyn ScheduleExecutor>,
        now: DateTime<Utc>,
    ) -> Self {
        let mut evaluated_through = HashMap::new();
        for (name, schedule) in &schedules {
            let grace = chrono::Duration::seconds(schedule.grace_seconds() as i64);
            let seeded = (now - grace)
                .with_timezone(&schedule.timezone())
                .naive_local();
            evaluated_through.insert(name.clone(), seeded);
        }

        Self {
            schedules,
            store,
            executor,
            evaluated_through,
        }
    }

    /// Evaluate every schedule once and return what happened.
    pub async fn evaluate(&mut self, now: DateTime<Utc>) -> Vec<Decision> {
        let mut names: Vec<String> = self.schedules.keys().cloned().collect();
        // Sorted so a tick's decisions are deterministic and testable.
        names.sort();

        let mut decisions = Vec::new();
        // Watermarks are applied after the loop: `decide` borrows `&self`, so
        // taking `&mut self` inside would conflict.
        let mut advanced: Vec<(String, NaiveDateTime)> = Vec::new();

        for name in &names {
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

            for instant in due {
                let outcome = self.decide(name, schedule, &instant, now).await;
                decisions.push(Decision {
                    schedule: name.clone(),
                    key: instant.key,
                    outcome,
                });
            }

            // Advance even when nothing was due, so the watermark tracks the
            // clock. On a fall-back date the local clock repeats and this stays
            // ahead for an hour; instants in the repeated hour already fired,
            // and their claims would refuse them anyway.
            if through > after {
                advanced.push((name.clone(), through));
            }
        }

        for (name, through) in advanced {
            self.evaluated_through.insert(name, through);
        }

        decisions
    }

    /// The per-instant decision order.
    ///
    /// Claiming first is what makes replicas agree: one process owns the
    /// instant, so two cannot reach different conclusions about it and both
    /// act. The cost is that a claimed instant skipped for grace, overlap, or
    /// capacity is burned rather than retried elsewhere — deliberate, so a
    /// saturated server does not build a backlog.
    async fn decide(
        &self,
        name: &str,
        schedule: &ScheduleConfig,
        instant: &timing::DueInstant,
        now: DateTime<Utc>,
    ) -> Outcome {
        match self
            .store
            .claim_schedule(name, &instant.key, schedule.claim_ttl_seconds())
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                // On N replicas this is the expected outcome N-1 times, so it
                // is debug rather than warn.
                tracing::debug!(schedule = %name, key = %instant.key, "instant claimed by a peer");
                return Outcome::NotClaimed;
            }
            Err(error) => {
                tracing::warn!(schedule = %name, key = %instant.key, %error, "claim failed");
                return Outcome::Failed {
                    error: error.to_string(),
                };
            }
        }

        let lateness_seconds = (now - instant.instant.with_timezone(&Utc)).num_seconds();
        if lateness_seconds > schedule.grace_seconds() as i64 {
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

        match self.executor.run(name, schedule).await {
            Ok(run_id) => {
                tracing::info!(schedule = %name, key = %instant.key, %run_id, "scheduled run started");
                Outcome::Fired { run_id }
            }
            // One broken schedule must never stop the others.
            Err(error) => {
                tracing::warn!(schedule = %name, key = %instant.key, %error, "scheduled run failed");
                Outcome::Failed { error }
            }
        }
    }
}
