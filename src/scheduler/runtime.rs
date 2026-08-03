//! Supervised scheduler task lifecycle for `ironflow serve`.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use chrono::Utc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use super::config::ScheduleConfig;
use super::{Scheduler, TICK_INTERVAL};
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

/// A scheduler task whose lifetime is tied to the API server.
pub struct SchedulerTask {
    shutdown: Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
    lifecycle: crate::api::ServiceLifecycle,
}

impl SchedulerTask {
    /// Stop the tick loop and wait until it has released its scheduler state.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.handle.await.map_err(join_error)
    }

    /// Run the API server and scheduler as one availability unit. An
    /// unexpected scheduler exit stops `serve`; normal server completion
    /// shuts the scheduler down deliberately.
    pub(crate) async fn supervise<F>(mut self, server: F) -> Result<()>
    where
        F: Future<Output = Result<()>>,
    {
        tokio::pin!(server);
        tokio::select! {
            server_result = &mut server => {
                self.lifecycle.begin_draining();
                if let Some(shutdown) = self.shutdown.take() {
                    let _ = shutdown.send(());
                }
                self.handle.await.map_err(join_error)?;
                server_result
            }
            task_result = &mut self.handle => {
                if !self.lifecycle.is_ready() {
                    task_result.map_err(join_error)?;
                    return server.await;
                }
                let error = match task_result {
                    Ok(()) => anyhow!("scheduler task stopped unexpectedly"),
                    Err(error) => join_error(error),
                };
                tracing::error!(%error, "scheduler task stopped; stopping API server");
                self.lifecycle.begin_draining();
                Err(error)
            }
        }
    }
}

fn join_error(error: tokio::task::JoinError) -> anyhow::Error {
    anyhow!("scheduler task failed: {error}")
}

/// Spawn the background tick loop, or `None` when nothing is scheduled.
///
/// Evaluates once immediately so a process restarting inside an instant's
/// grace window does not wait a full tick.
pub fn spawn(
    schedules: HashMap<String, ScheduleConfig>,
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    flows_dir: Option<PathBuf>,
    max_concurrent_tasks: Option<usize>,
) -> Option<SchedulerTask> {
    spawn_with_lifecycle(
        schedules,
        store,
        event_store,
        flows_dir,
        max_concurrent_tasks,
        crate::api::ServiceLifecycle::default(),
    )
}

pub(crate) fn spawn_with_lifecycle(
    schedules: HashMap<String, ScheduleConfig>,
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    flows_dir: Option<PathBuf>,
    max_concurrent_tasks: Option<usize>,
    lifecycle: crate::api::ServiceLifecycle,
) -> Option<SchedulerTask> {
    if schedules.is_empty() {
        return None;
    }

    let mut names: Vec<&str> = schedules.keys().map(String::as_str).collect();
    names.sort_unstable();
    tracing::info!(
        count = schedules.len(),
        schedules = %names.join(", "),
        tick_seconds = TICK_INTERVAL.as_secs(),
        "scheduler started"
    );

    let executor = Arc::new(super::execution::FlowExecutor::new_with_lifecycle(
        Arc::new(crate::nodes::NodeRegistry::with_builtins()),
        store.clone(),
        event_store,
        flows_dir,
        max_concurrent_tasks,
        lifecycle.clone(),
    ));
    let mut scheduler = Scheduler::new(schedules, store, executor, Utc::now());
    let (shutdown, mut shutdown_requested) = oneshot::channel();
    let task_lifecycle = lifecycle.clone();
    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(TICK_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    scheduler.evaluate(Utc::now()).await;
                }
                _ = &mut shutdown_requested => {
                    tracing::info!("scheduler stopped");
                    return;
                }
                _ = task_lifecycle.wait_for_draining() => {
                    tracing::info!("scheduler stopped for service drain");
                    return;
                }
            }
        }
    });

    Some(SchedulerTask {
        shutdown: Some(shutdown),
        handle,
        lifecycle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(handle: JoinHandle<()>, shutdown: oneshot::Sender<()>) -> SchedulerTask {
        SchedulerTask {
            shutdown: Some(shutdown),
            handle,
            lifecycle: crate::api::ServiceLifecycle::default(),
        }
    }

    #[tokio::test]
    async fn an_unexpected_task_exit_fails_supervision() {
        let (shutdown, _receiver) = oneshot::channel();
        let scheduler = task(tokio::spawn(async {}), shutdown);
        let result = scheduler
            .supervise(std::future::pending::<Result<()>>())
            .await;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("stopped unexpectedly")
        );
    }

    #[tokio::test]
    async fn server_completion_requests_and_awaits_scheduler_shutdown() {
        let (shutdown, receiver) = oneshot::channel();
        let handle = tokio::spawn(async move {
            receiver.await.unwrap();
        });
        task(handle, shutdown)
            .supervise(async { Ok(()) })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_panicking_task_fails_supervision() {
        let (shutdown, _receiver) = oneshot::channel();
        let scheduler = task(
            tokio::spawn(async { panic!("injected scheduler panic") }),
            shutdown,
        );
        let error = scheduler
            .supervise(std::future::pending::<Result<()>>())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("panicked"), "{error}");
    }
}
