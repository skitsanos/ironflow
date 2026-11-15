//! Recording of single observations into the bounded metric families.

use std::time::Duration;

use super::storage_labels::storage_error_kind;
use super::{
    AdmissionDecision, AdmissionLabels, AdmissionResource, EventAdmissionOutcome, LeaseOutcome,
    Metrics, OutcomeLabels, RunOutcome, SchedulerOutcome, StorageLabels, StorageOperation,
    StoreKind, TaskOutcome,
};

impl Metrics {
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

    pub(super) fn record_run(&self, outcome: RunOutcome, duration: Duration) {
        let labels = OutcomeLabels {
            outcome: outcome.as_str(),
        };
        self.runs.get_or_create(&labels).inc();
        self.run_duration
            .get_or_create(&labels)
            .observe(duration.as_secs_f64());
    }

    pub(super) fn record_task_attempt(&self, outcome: TaskOutcome, duration: Duration) {
        let labels = OutcomeLabels {
            outcome: outcome.as_str(),
        };
        self.task_attempts.get_or_create(&labels).inc();
        self.task_attempt_duration
            .get_or_create(&labels)
            .observe(duration.as_secs_f64());
    }

    pub(crate) fn event_admission(&self, outcome: EventAdmissionOutcome) {
        self.event_admissions
            .get_or_create(&OutcomeLabels {
                outcome: outcome.as_str(),
            })
            .inc();
    }
}
