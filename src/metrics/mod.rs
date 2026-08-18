mod event_store;
mod labels;
mod observation;
mod state_store;
mod storage_labels;

use std::sync::Arc;
use std::time::Duration;

use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::{Family, MetricConstructor};
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::registry::{Registry, Unit};

pub(crate) use event_store::observe_event_store;
use labels::RunOutcome;
pub(crate) use labels::{
    ActiveWorkKind, AdmissionDecision, AdmissionResource, LeaseOutcome, SchedulerOutcome,
    TaskOutcome,
};
pub(crate) use observation::{ActiveWorkGuard, RunObservation, TaskAttemptObservation};
pub(crate) use state_store::observe_state_store;
use storage_labels::{STORAGE_ERROR_KINDS, storage_error_kind};
pub(crate) use storage_labels::{StorageOperation, StoreKind};

const DURATION_BUCKETS: [f64; 14] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0,
];

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct OutcomeLabels {
    outcome: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct ActiveLabels {
    kind: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct AdmissionLabels {
    resource: &'static str,
    decision: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct StorageLabels {
    store: &'static str,
    operation: &'static str,
    error_kind: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct DurationHistogram;

impl MetricConstructor<Histogram> for DurationHistogram {
    fn new_metric(&self) -> Histogram {
        Histogram::new(DURATION_BUCKETS)
    }
}

type DurationFamily = Family<OutcomeLabels, Histogram, DurationHistogram>;

/// One process-local, bounded-cardinality metrics registry.
#[derive(Debug)]
pub struct Metrics {
    registry: Registry,
    runs: Family<OutcomeLabels, Counter>,
    run_duration: DurationFamily,
    task_attempts: Family<OutcomeLabels, Counter>,
    task_attempt_duration: DurationFamily,
    active_work: Family<ActiveLabels, Gauge>,
    admission_decisions: Family<AdmissionLabels, Counter>,
    scheduler_decisions: Family<OutcomeLabels, Counter>,
    lease_events: Family<OutcomeLabels, Counter>,
    storage_failures: Family<StorageLabels, Counter>,
}

impl Metrics {
    pub(crate) fn new() -> Self {
        let mut registry = Registry::with_prefix("ironflow");
        let runs = Family::default();
        let run_duration = Family::new_with_constructor(DurationHistogram);
        let task_attempts = Family::default();
        let task_attempt_duration = Family::new_with_constructor(DurationHistogram);
        let active_work = Family::default();
        let admission_decisions = Family::default();
        let scheduler_decisions = Family::default();
        let lease_events = Family::default();
        let storage_failures = Family::default();

        registry.register("runs", "Terminal workflow runs by outcome", runs.clone());
        registry.register_with_unit(
            "run_duration",
            "Workflow run duration from durable initialization through terminalization",
            Unit::Seconds,
            run_duration.clone(),
        );
        registry.register(
            "task_attempts",
            "Workflow task attempts by outcome",
            task_attempts.clone(),
        );
        registry.register_with_unit(
            "task_attempt_duration",
            "Workflow task attempt execution duration",
            Unit::Seconds,
            task_attempt_duration.clone(),
        );
        registry.register(
            "active_work",
            "Current process-local workflow work by bounded kind",
            active_work.clone(),
        );
        registry.register(
            "admission_decisions",
            "Process-local admission decisions by resource and result",
            admission_decisions.clone(),
        );
        registry.register(
            "scheduler_decisions",
            "Schedule occurrence decisions by bounded outcome",
            scheduler_decisions.clone(),
        );
        registry.register(
            "lease_events",
            "Workflow ownership lease events by bounded outcome",
            lease_events.clone(),
        );
        registry.register(
            "storage_failures",
            "State and event store operation failures by bounded category",
            storage_failures.clone(),
        );

        let metrics = Self {
            registry,
            runs,
            run_duration,
            task_attempts,
            task_attempt_duration,
            active_work,
            admission_decisions,
            scheduler_decisions,
            lease_events,
            storage_failures,
        };
        metrics.initialize_bounded_series();
        metrics
    }

    fn initialize_bounded_series(&self) {
        for outcome in RunOutcome::ALL {
            let labels = OutcomeLabels {
                outcome: outcome.as_str(),
            };
            let _ = self.runs.get_or_create(&labels);
            let _ = self.run_duration.get_or_create(&labels);
        }
        for outcome in TaskOutcome::ALL {
            let labels = OutcomeLabels {
                outcome: outcome.as_str(),
            };
            let _ = self.task_attempts.get_or_create(&labels);
            let _ = self.task_attempt_duration.get_or_create(&labels);
        }
        for kind in ActiveWorkKind::ALL {
            let _ = self.active_work.get_or_create(&ActiveLabels {
                kind: kind.as_str(),
            });
        }
        for resource in AdmissionResource::ALL {
            for decision in AdmissionDecision::ALL {
                let _ = self.admission_decisions.get_or_create(&AdmissionLabels {
                    resource: resource.as_str(),
                    decision: decision.as_str(),
                });
            }
        }
        for outcome in SchedulerOutcome::ALL {
            let _ = self.scheduler_decisions.get_or_create(&OutcomeLabels {
                outcome: outcome.as_str(),
            });
        }
        for outcome in LeaseOutcome::ALL {
            let _ = self.lease_events.get_or_create(&OutcomeLabels {
                outcome: outcome.as_str(),
            });
        }
        for operation in StorageOperation::STATE {
            self.initialize_storage_series(StoreKind::State, operation);
        }
        for operation in StorageOperation::EVENT {
            self.initialize_storage_series(StoreKind::Event, operation);
        }
    }

    fn initialize_storage_series(&self, store: StoreKind, operation: StorageOperation) {
        for error_kind in STORAGE_ERROR_KINDS {
            let _ = self.storage_failures.get_or_create(&StorageLabels {
                store: store.as_str(),
                operation: operation.as_str(),
                error_kind,
            });
        }
    }

    pub(crate) fn encode(&self) -> Result<String, std::fmt::Error> {
        let mut output = String::new();
        encode(&mut output, &self.registry)?;
        Ok(output)
    }

    pub(crate) fn run_observation(self: &Arc<Self>) -> Arc<RunObservation> {
        Arc::new(RunObservation::new(self.clone()))
    }

    pub(crate) fn task_attempt(self: &Arc<Self>) -> TaskAttemptObservation {
        TaskAttemptObservation::new(self.clone())
    }

    pub(crate) fn active_work(self: &Arc<Self>, kind: ActiveWorkKind) -> ActiveWorkGuard {
        self.active_work
            .get_or_create(&ActiveLabels {
                kind: kind.as_str(),
            })
            .inc();
        ActiveWorkGuard::new(self.clone(), kind)
    }

    pub(crate) fn admission(&self, resource: AdmissionResource, decision: AdmissionDecision) {
        self.admission_decisions
            .get_or_create(&AdmissionLabels {
                resource: resource.as_str(),
                decision: decision.as_str(),
            })
            .inc();
    }

    pub(crate) fn scheduler(&self, outcome: SchedulerOutcome) {
        self.scheduler_decisions
            .get_or_create(&OutcomeLabels {
                outcome: outcome.as_str(),
            })
            .inc();
    }

    pub(crate) fn lease(&self, outcome: LeaseOutcome) {
        self.lease_events
            .get_or_create(&OutcomeLabels {
                outcome: outcome.as_str(),
            })
            .inc();
    }

    pub(crate) fn storage_failure(
        &self,
        store: StoreKind,
        operation: StorageOperation,
        kind: crate::storage::StorageErrorKind,
    ) {
        self.storage_failures
            .get_or_create(&StorageLabels {
                store: store.as_str(),
                operation: operation.as_str(),
                error_kind: storage_error_kind(kind),
            })
            .inc();
    }

    fn record_run(&self, outcome: RunOutcome, duration: Duration) {
        let labels = OutcomeLabels {
            outcome: outcome.as_str(),
        };
        self.runs.get_or_create(&labels).inc();
        self.run_duration
            .get_or_create(&labels)
            .observe(duration.as_secs_f64());
    }

    fn record_task_attempt(&self, outcome: TaskOutcome, duration: Duration) {
        let labels = OutcomeLabels {
            outcome: outcome.as_str(),
        };
        self.task_attempts.get_or_create(&labels).inc();
        self.task_attempt_duration
            .get_or_create(&labels)
            .observe(duration.as_secs_f64());
    }
}

#[cfg(test)]
mod tests;
