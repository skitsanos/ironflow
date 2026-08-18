use crate::storage::StorageErrorKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoreKind {
    State,
    Event,
}

impl StoreKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Event => "event",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageOperation {
    Healthcheck,
    InitRun,
    InitRunOwned,
    SetRunStatus,
    SetRunStatusOwned,
    RenewRunLease,
    ReconcileExpiredRunLeases,
    UpsertTask,
    UpsertTaskOwned,
    GetContext,
    UpdateContext,
    UpdateContextOwned,
    GetRunInfo,
    ListRuns,
    ListRunSummaries,
    ListRunSummariesPage,
    DeleteRun,
    PruneBefore,
    ClaimSchedule,
    PublishEvent,
    ListEvents,
}

impl StorageOperation {
    pub(crate) const STATE: [Self; 19] = [
        Self::Healthcheck,
        Self::InitRun,
        Self::InitRunOwned,
        Self::SetRunStatus,
        Self::SetRunStatusOwned,
        Self::RenewRunLease,
        Self::ReconcileExpiredRunLeases,
        Self::UpsertTask,
        Self::UpsertTaskOwned,
        Self::GetContext,
        Self::UpdateContext,
        Self::UpdateContextOwned,
        Self::GetRunInfo,
        Self::ListRuns,
        Self::ListRunSummaries,
        Self::ListRunSummariesPage,
        Self::DeleteRun,
        Self::PruneBefore,
        Self::ClaimSchedule,
    ];

    pub(crate) const EVENT: [Self; 4] = [
        Self::Healthcheck,
        Self::PublishEvent,
        Self::DeleteRun,
        Self::ListEvents,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Healthcheck => "healthcheck",
            Self::InitRun => "init_run",
            Self::InitRunOwned => "init_run_owned",
            Self::SetRunStatus => "set_run_status",
            Self::SetRunStatusOwned => "set_run_status_owned",
            Self::RenewRunLease => "renew_run_lease",
            Self::ReconcileExpiredRunLeases => "reconcile_expired_run_leases",
            Self::UpsertTask => "upsert_task",
            Self::UpsertTaskOwned => "upsert_task_owned",
            Self::GetContext => "get_context",
            Self::UpdateContext => "update_context",
            Self::UpdateContextOwned => "update_context_owned",
            Self::GetRunInfo => "get_run_info",
            Self::ListRuns => "list_runs",
            Self::ListRunSummaries => "list_run_summaries",
            Self::ListRunSummariesPage => "list_run_summaries_page",
            Self::DeleteRun => "delete_run",
            Self::PruneBefore => "prune_before",
            Self::ClaimSchedule => "claim_schedule",
            Self::PublishEvent => "publish_event",
            Self::ListEvents => "list_events",
        }
    }
}

pub(crate) const STORAGE_ERROR_KINDS: [&str; 5] = [
    "invalid_input",
    "not_found",
    "backend",
    "corruption",
    "conflict",
];

pub(crate) const fn storage_error_kind(kind: StorageErrorKind) -> &'static str {
    match kind {
        StorageErrorKind::InvalidInput => "invalid_input",
        StorageErrorKind::NotFound => "not_found",
        StorageErrorKind::Backend => "backend",
        StorageErrorKind::Corruption => "corruption",
        StorageErrorKind::Conflict => "conflict",
    }
}
