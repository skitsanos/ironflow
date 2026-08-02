use std::sync::Arc;

use tokio::sync::watch;

use crate::engine::types::RunStatus;

/// Durable meaning attached to a request to stop local execution.
///
/// Infrastructure failures outrank cancellation so a lease failure cannot be
/// overwritten by a simultaneous user request or run deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ExecutionSignal {
    Running,
    Cancelled,
    Infrastructure(Arc<str>),
}

pub(super) enum ExecutionOutcome {
    Completed(RunStatus),
    Cancelled,
    Infrastructure(anyhow::Error),
}

impl ExecutionSignal {
    pub(super) fn outcome(&self) -> Option<ExecutionOutcome> {
        match self {
            Self::Running => None,
            Self::Cancelled => Some(ExecutionOutcome::Cancelled),
            Self::Infrastructure(reason) => Some(ExecutionOutcome::Infrastructure(
                anyhow::anyhow!(reason.to_string()),
            )),
        }
    }
}

pub(super) fn request_cancellation(signal: &watch::Sender<ExecutionSignal>) {
    signal.send_if_modified(|current| {
        if matches!(current, ExecutionSignal::Running) {
            *current = ExecutionSignal::Cancelled;
            true
        } else {
            false
        }
    });
}

pub(super) fn report_infrastructure(
    signal: &watch::Sender<ExecutionSignal>,
    reason: impl Into<Arc<str>>,
) {
    let reason = reason.into();
    signal.send_if_modified(|current| {
        if matches!(current, ExecutionSignal::Infrastructure(_)) {
            false
        } else {
            *current = ExecutionSignal::Infrastructure(reason);
            true
        }
    });
}

pub(super) async fn stop_requested(
    signal: &mut watch::Receiver<ExecutionSignal>,
) -> ExecutionOutcome {
    loop {
        if let Some(outcome) = signal.borrow().outcome() {
            return outcome;
        }
        if signal.changed().await.is_err() {
            return ExecutionOutcome::Infrastructure(anyhow::anyhow!(
                "workflow execution control channel closed unexpectedly"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infrastructure_stop_cannot_be_downgraded_to_cancellation() {
        let (signal, receiver) = watch::channel(ExecutionSignal::Running);
        request_cancellation(&signal);
        report_infrastructure(&signal, "lease renewal failed");
        request_cancellation(&signal);

        assert_eq!(
            *receiver.borrow(),
            ExecutionSignal::Infrastructure(Arc::from("lease renewal failed"))
        );
    }
}
