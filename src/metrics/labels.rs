use crate::engine::types::RunStatus;
use crate::scheduler::Outcome;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunOutcome {
    Success,
    Failed,
    Cancelled,
    Stalled,
}

impl RunOutcome {
    pub(crate) const ALL: [Self; 4] = [Self::Success, Self::Failed, Self::Cancelled, Self::Stalled];

    pub(crate) fn from_status(status: &RunStatus) -> Option<Self> {
        match status {
            RunStatus::Success => Some(Self::Success),
            RunStatus::Failed => Some(Self::Failed),
            RunStatus::Cancelled => Some(Self::Cancelled),
            RunStatus::Stalled => Some(Self::Stalled),
            RunStatus::Pending | RunStatus::Running => None,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Stalled => "stalled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskOutcome {
    Success,
    Failed,
    TimedOut,
    Aborted,
}

impl TaskOutcome {
    pub(crate) const ALL: [Self; 4] = [Self::Success, Self::Failed, Self::TimedOut, Self::Aborted];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Aborted => "aborted",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveWorkKind {
    Run,
    Task,
    FlowLoad,
}

impl ActiveWorkKind {
    pub(crate) const ALL: [Self; 3] = [Self::Run, Self::Task, Self::FlowLoad];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Task => "task",
            Self::FlowLoad => "flow_load",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdmissionResource {
    Run,
    FlowLoad,
}

impl AdmissionResource {
    pub(crate) const ALL: [Self; 2] = [Self::Run, Self::FlowLoad];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::FlowLoad => "flow_load",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdmissionDecision {
    Accepted,
    AtCapacity,
    Draining,
}

impl AdmissionDecision {
    pub(crate) const ALL: [Self; 3] = [Self::Accepted, Self::AtCapacity, Self::Draining];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::AtCapacity => "at_capacity",
            Self::Draining => "draining",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchedulerOutcome {
    Fired,
    NotClaimed,
    Late,
    Overlapped,
    AtCapacity,
    Failed,
    ClaimFailed,
    TimedOut,
}

impl SchedulerOutcome {
    pub(crate) const ALL: [Self; 8] = [
        Self::Fired,
        Self::NotClaimed,
        Self::Late,
        Self::Overlapped,
        Self::AtCapacity,
        Self::Failed,
        Self::ClaimFailed,
        Self::TimedOut,
    ];

    pub(crate) fn from_outcome(outcome: &Outcome) -> Self {
        match outcome {
            Outcome::Fired { .. } => Self::Fired,
            Outcome::NotClaimed => Self::NotClaimed,
            Outcome::Late { .. } => Self::Late,
            Outcome::Overlapped { .. } => Self::Overlapped,
            Outcome::AtCapacity => Self::AtCapacity,
            Outcome::Failed { .. } => Self::Failed,
            Outcome::ClaimFailed { .. } => Self::ClaimFailed,
            Outcome::TimedOut => Self::TimedOut,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Fired => "fired",
            Self::NotClaimed => "not_claimed",
            Self::Late => "late",
            Self::Overlapped => "overlapped",
            Self::AtCapacity => "at_capacity",
            Self::Failed => "failed",
            Self::ClaimFailed => "claim_failed",
            Self::TimedOut => "timed_out",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeaseOutcome {
    Renewed,
    Lost,
    TimedOut,
    Error,
    ReconciliationFailed,
}

impl LeaseOutcome {
    pub(crate) const ALL: [Self; 5] = [
        Self::Renewed,
        Self::Lost,
        Self::TimedOut,
        Self::Error,
        Self::ReconciliationFailed,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Renewed => "renewed",
            Self::Lost => "lost",
            Self::TimedOut => "timed_out",
            Self::Error => "error",
            Self::ReconciliationFailed => "reconciliation_failed",
        }
    }
}
