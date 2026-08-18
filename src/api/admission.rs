//! Process-wide admission control for API and scheduler initiated runs.

use std::sync::{Arc, OnceLock};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::ServiceLifecycle;
use super::errors::AppError;
use crate::engine::RunHandle;

type AdmissionConfig = Result<Option<Arc<Semaphore>>, String>;
type RequiredAdmissionConfig = Result<Arc<Semaphore>, String>;

fn configured_semaphore() -> anyhow::Result<Option<&'static Arc<Semaphore>>> {
    static SEMAPHORE: OnceLock<AdmissionConfig> = OnceLock::new();

    match SEMAPHORE.get_or_init(|| {
        crate::util::runtime_config::max_concurrent_runs()
            .map(|limit| limit.map(|limit| Arc::new(Semaphore::new(limit))))
            .map_err(|error| error.to_string())
    }) {
        Ok(semaphore) => Ok(semaphore.as_ref()),
        Err(message) => Err(anyhow::anyhow!(message.clone())),
    }
}

fn configured_flow_load_semaphore() -> anyhow::Result<&'static Arc<Semaphore>> {
    static SEMAPHORE: OnceLock<RequiredAdmissionConfig> = OnceLock::new();

    match SEMAPHORE.get_or_init(|| {
        crate::util::runtime_config::max_concurrent_flow_loads()
            .map(|limit| Arc::new(Semaphore::new(limit)))
            .map_err(|error| error.to_string())
    }) {
        Ok(semaphore) => Ok(semaphore),
        Err(message) => Err(anyhow::anyhow!(message.clone())),
    }
}

/// Resolve admission configuration before the server binds or schedules work.
pub(super) fn validate_configuration() -> anyhow::Result<()> {
    let _ = configured_semaphore()?;
    let _ = configured_flow_load_semaphore()?;
    Ok(())
}

/// Acquire a permit held for an API- or scheduler-triggered run's duration.
///
/// `None` means the cap is explicitly unlimited. An invalid configured limit
/// is an internal configuration failure, never an implicit unlimited mode.
pub(crate) fn acquire_run_permit(
    lifecycle: &ServiceLifecycle,
    metrics: Option<&crate::metrics::Metrics>,
) -> Result<Option<OwnedSemaphorePermit>, AppError> {
    if !lifecycle.is_ready() {
        if let Some(metrics) = metrics {
            metrics.admission(
                crate::metrics::AdmissionResource::Run,
                crate::metrics::AdmissionDecision::Draining,
            );
        }
        return Err(AppError::ServiceUnavailable(
            "server is draining; retry on another replica".to_string(),
        ));
    }
    let semaphore = configured_semaphore().map_err(AppError::Internal)?;
    let result = acquire_run_permit_from(semaphore);
    if let Some(metrics) = metrics {
        let decision = if result.is_ok() {
            crate::metrics::AdmissionDecision::Accepted
        } else {
            crate::metrics::AdmissionDecision::AtCapacity
        };
        metrics.admission(crate::metrics::AdmissionResource::Run, decision);
    }
    result
}

/// Reserve one of the bounded Lua flow-definition evaluation slots.
///
/// API requests fail fast rather than accumulating request bodies while they
/// wait for a potentially long-running parse to finish.
pub(crate) fn acquire_flow_load_permit(
    metrics: Option<&crate::metrics::Metrics>,
) -> Result<OwnedSemaphorePermit, AppError> {
    let semaphore = configured_flow_load_semaphore().map_err(AppError::Internal)?;
    let result = semaphore.clone().try_acquire_owned().map_err(|_| {
        AppError::ServiceUnavailable(
            "server is at maximum concurrent flow-loading capacity; retry later".to_string(),
        )
    });
    if let Some(metrics) = metrics {
        let decision = if result.is_ok() {
            crate::metrics::AdmissionDecision::Accepted
        } else {
            crate::metrics::AdmissionDecision::AtCapacity
        };
        metrics.admission(crate::metrics::AdmissionResource::FlowLoad, decision);
    }
    result
}

/// Run an admitted flow parse in a detached supervisor that owns its permit.
///
/// Flow loading delegates synchronous work to a blocking worker. If an HTTP
/// waiter disappears while that work is inside a non-interruptible read or C
/// call, dropping the waiter does not guarantee the worker has stopped. The
/// detached task therefore retains admission until the load truly settles.
pub(crate) async fn supervise_flow_load<T, F>(
    permit: OwnedSemaphorePermit,
    load: F,
    metrics: Option<Arc<crate::metrics::Metrics>>,
) -> anyhow::Result<T>
where
    T: Send + 'static,
    F: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
{
    let task = tokio::spawn(async move {
        let _active_work =
            metrics.map(|metrics| metrics.active_work(crate::metrics::ActiveWorkKind::FlowLoad));
        let _permit = permit;
        load.await
    });
    task.await
        .map_err(|error| anyhow::anyhow!("flow-loading supervisor stopped unexpectedly: {error}"))?
}

/// Wait for a started run while retaining process admission independently of
/// the HTTP request future. `RunHandle` deliberately detaches on drop, so the
/// permit must be owned by an equally detached waiter.
pub(crate) async fn wait_for_admitted_run(
    lifecycle: ServiceLifecycle,
    handle: RunHandle,
    permit: Option<OwnedSemaphorePermit>,
) -> anyhow::Result<String> {
    let active_run = lifecycle.track(&handle);
    let task = tokio::spawn(async move {
        let _active_run = active_run;
        let _permit = permit;
        handle.wait().await
    });
    task.await
        .map_err(|error| anyhow::anyhow!("admitted run supervisor stopped unexpectedly: {error}"))?
}

fn acquire_run_permit_from(
    semaphore: Option<&Arc<Semaphore>>,
) -> Result<Option<OwnedSemaphorePermit>, AppError> {
    match semaphore {
        None => Ok(None),
        Some(semaphore) => semaphore
            .clone()
            .try_acquire_owned()
            .map(Some)
            .map_err(|_| {
                AppError::ServiceUnavailable(
                    "server is at maximum concurrent run capacity; retry later".to_string(),
                )
            }),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn permit_gating_tracks_capacity() {
        assert!(super::acquire_run_permit_from(None).unwrap().is_none());

        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
        let first = super::acquire_run_permit_from(Some(&semaphore)).unwrap();
        assert!(first.is_some());
        assert!(super::acquire_run_permit_from(Some(&semaphore)).is_err());
        drop(first);
        assert!(
            super::acquire_run_permit_from(Some(&semaphore))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn draining_admission_is_recorded_without_exposing_request_data() {
        let lifecycle = crate::api::ServiceLifecycle::default();
        lifecycle.begin_draining();
        let metrics = crate::metrics::Metrics::new();

        assert!(super::acquire_run_permit(&lifecycle, Some(&metrics)).is_err());
        let encoded = metrics.encode().unwrap();
        assert!(encoded.contains(
            "ironflow_admission_decisions_total{resource=\"run\",decision=\"draining\"} 1"
        ));
    }
}
