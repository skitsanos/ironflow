//! Cron-driven flow triggers for `ironflow serve`.

mod catchup;
pub mod config;
mod cron;
mod evaluation;
pub mod execution;
mod identity;
mod runtime;
pub mod startup;
pub mod timing;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};

use crate::storage::StateStore;

use config::ScheduleConfig;

pub(crate) use runtime::spawn_with_lifecycle;
pub use runtime::{SchedulerTask, spawn};

/// How often the scheduler evaluates every schedule.
///
/// Cron's finest granularity here is one minute, so a 30-second tick cannot
/// miss a minute boundary.
pub const TICK_INTERVAL: Duration = Duration::from_secs(30);

/// Maximum wall time one schedule may consume during a tick.
///
/// Names are evaluated concurrently, so a timeout burns only the affected
/// schedule's due instants and cannot hold unrelated schedules indefinitely.
pub const SCHEDULE_EVALUATION_TIMEOUT: Duration = Duration::from_secs(15);

/// What happened to one due instant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The instant was owned and the run was started.
    Fired { run_id: String },
    /// Another replica owns this instant.
    NotClaimed,
    /// Past the grace window.
    Late { lateness_seconds: i64 },
    /// A previous run of this schedule has not finished.
    Overlapped { active_run: String },
    /// The process is at `IRONFLOW_MAX_CONCURRENT_RUNS`.
    AtCapacity,
    /// The flow could not be started (bad path, load failure, or engine
    /// error). This run never started; the scheduler did not stop.
    Failed { error: String },
    /// The store errored while claiming the instant. Distinct from `Failed`:
    /// no run was ever owned, so `evaluate` must not treat the instant as
    /// settled — it is retried, not burned.
    ClaimFailed { error: String },
    /// Evaluation exceeded its budget. The claim or run-start result can be
    /// indeterminate, so the instant is burned rather than retried.
    TimedOut,
}

/// Durable start result for one deterministic schedule occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleRun {
    Started { run_id: String },
    Existing { run_id: String },
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

    /// Start the schedule's flow and return its run id, without waiting for
    /// the run to finish.
    async fn run(
        &self,
        schedule_name: &str,
        instant_key: &str,
        schedule: &ScheduleConfig,
    ) -> Result<ScheduleRun, String>;
}

/// Evaluates every schedule on a tick.
pub struct Scheduler {
    schedules: HashMap<String, ScheduleConfig>,
    store: Arc<dyn StateStore>,
    executor: Arc<dyn ScheduleExecutor>,
    /// Local wall-clock time each schedule was last evaluated through.
    evaluated_through: HashMap<String, NaiveDateTime>,
    evaluation_timeout: Duration,
}

impl Scheduler {
    /// Seeds each schedule's watermark at `timing::grace_floor` (`now -
    /// grace_seconds`), so a process that starts after an outage catches up
    /// only as far as an instant could still legitimately fire, rather than
    /// replaying history.
    pub fn new(
        schedules: HashMap<String, ScheduleConfig>,
        store: Arc<dyn StateStore>,
        executor: Arc<dyn ScheduleExecutor>,
        now: DateTime<Utc>,
    ) -> Self {
        let mut evaluated_through = HashMap::new();
        for (name, schedule) in &schedules {
            let seeded = timing::grace_floor(now, schedule);
            catchup::log_unreachable_catchup(name, schedule, seeded);
            evaluated_through.insert(name.clone(), seeded);
        }

        Self {
            schedules,
            store,
            executor,
            evaluated_through,
            evaluation_timeout: SCHEDULE_EVALUATION_TIMEOUT,
        }
    }

    /// Override the per-schedule evaluation budget for embedded runtimes and
    /// deterministic fault-injection tests. The CLI uses the 15-second
    /// default, which is shorter than one scheduler tick.
    pub fn with_evaluation_timeout(mut self, timeout: Duration) -> Result<Self, String> {
        if timeout.is_zero() {
            return Err("scheduler evaluation timeout must be positive".to_string());
        }
        self.evaluation_timeout = timeout;
        Ok(self)
    }
}
