//! IF-201: a scheduled occurrence whose run record is gone must not start
//! again inside its grace window, while a claim that never produced a run is
//! still recovered (IF-078).
//!
//! Every case drives the real `Scheduler` and `FlowExecutor` over a real
//! store. The flow's only step is a counting node, so a second execution of
//! the same instant is observed directly rather than inferred from a record.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::{DateTime, TimeZone as _, Utc};
use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::execution::FlowExecutor;
use ironflow::scheduler::{Outcome, ScheduleExecutor, ScheduleRun, Scheduler};
use ironflow::storage::StateStore;
use ironflow::storage::event_store::MemoryEventStore;

use super::test_scheduler_e2e::PROCESS_ADMISSION_LOCK;

#[cfg(feature = "redis")]
#[path = "support/redis.rs"]
mod redis_support;

#[cfg(feature = "postgres")]
#[path = "support/postgres.rs"]
mod postgres_support;

const NAME: &str = "nightly";
const KEY: &str = "UTC@2026-05-01T02:00";
const CLAIM_TTL: u64 = 604_800;
const SETTLE: Duration = Duration::from_secs(60);

#[derive(Default)]
struct CountingNode {
    executions: AtomicUsize,
}

#[async_trait::async_trait]
impl ironflow::nodes::Node for CountingNode {
    fn node_type(&self) -> &str {
        "delay"
    }

    fn description(&self) -> &str {
        "Counts executions of the scheduled flow"
    }

    async fn execute(
        &self,
        _config: &serde_json::Value,
        _ctx: &Context,
    ) -> anyhow::Result<NodeOutput> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        Ok(NodeOutput::new())
    }
}

/// One store shared by every "replica" a case starts.
struct Deployment {
    store: Arc<dyn StateStore>,
    flows: tempfile::TempDir,
    counter: Arc<CountingNode>,
}

impl Deployment {
    fn new(store: Arc<dyn StateStore>) -> Self {
        let flows = tempfile::tempdir().unwrap();
        std::fs::write(
            flows.path().join("nightly.lua"),
            r#"
            local flow = Flow.new("nightly_report")
            flow:step("work", nodes.delay({ seconds = 1 }))
            return flow
            "#,
        )
        .unwrap();
        Self {
            store,
            flows,
            counter: Arc::new(CountingNode::default()),
        }
    }

    fn schedule() -> ScheduleConfig {
        // One day of grace: every evaluation below is far inside it.
        ScheduleConfig::new(
            "nightly.lua",
            "0 2 * * *",
            Some("UTC"),
            Some(86_400),
            Context::new(),
        )
        .unwrap()
    }

    fn executor(&self) -> Arc<FlowExecutor> {
        let mut registry = NodeRegistry::with_builtins();
        registry.register(self.counter.clone());
        Arc::new(FlowExecutor::new(
            Arc::new(registry),
            self.store.clone(),
            Arc::new(MemoryEventStore::new()),
            Some(self.flows.path().to_path_buf()),
            None,
        ))
    }

    /// A replica that starts at `started` and therefore catches up over the
    /// preceding grace window, exactly like a restarted process.
    fn replica(&self, started: DateTime<Utc>) -> Scheduler {
        Scheduler::new(
            HashMap::from([(NAME.to_string(), Self::schedule())]),
            self.store.clone(),
            self.executor(),
            started,
        )
    }

    /// Evaluate one tick and return the outcome for the 02:00 instant. The
    /// process-wide flow-load permits are shared with neighbouring tests, so
    /// the lock is held for the evaluation only, never across a wait.
    async fn evaluate(&self, scheduler: &mut Scheduler, now: DateTime<Utc>) -> Outcome {
        let _admission = PROCESS_ADMISSION_LOCK.lock().await;
        let decisions = scheduler.evaluate(now).await;
        let decision = decisions
            .into_iter()
            .find(|decision| decision.key == KEY)
            .expect("the 02:00 instant must be evaluated");
        decision.outcome
    }

    async fn fire_first_occurrence(&self) -> String {
        // Seeded exactly one grace window (a day) before 02:00, so yesterday's
        // 02:00 is outside the half-open catch-up interval and only today's
        // instant is due.
        let mut first = self.replica(at(2, 0, 0));
        let Outcome::Fired { run_id } = self.evaluate(&mut first, at(2, 0, 0)).await else {
            panic!("the first replica must start the occurrence");
        };
        self.wait_for_count(1).await;
        run_id
    }

    async fn wait_for_count(&self, expected: usize) {
        poll("the scheduled flow to execute", || async {
            self.counter.executions.load(Ordering::SeqCst) >= expected
        })
        .await;
    }

    /// Poll until the run is terminal, or already gone on a store that
    /// expires records.
    async fn wait_for_terminal_or_gone(&self, run_id: &str) {
        poll("the run to finish", || async {
            match self.store.get_run_info(run_id).await {
                Ok(info) => info.status.is_terminal(),
                Err(error) => error.is_not_found(),
            }
        })
        .await;
    }

    async fn wait_for_gone(&self, run_id: &str) {
        poll("the run record to disappear", || async {
            matches!(self.store.get_run_info(run_id).await, Err(error) if error.is_not_found())
        })
        .await;
    }

    async fn run_count(&self) -> usize {
        self.store.list_runs(None).await.unwrap().len()
    }

    fn executions(&self) -> usize {
        self.counter.executions.load(Ordering::SeqCst)
    }
}

fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 1, hour, minute, second)
        .unwrap()
}

/// Poll state instead of sleeping a fixed time, so a loaded machine only
/// makes a case slower, never wrong (IF-216).
async fn poll<F, Fut>(what: &str, mut ready: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + SETTLE;
    while !ready().await {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// A restarted replica inside the grace window must not run an occurrence
/// again after its record was deleted: `DELETE /runs/{id}` removes the
/// record, it does not release the instant.
async fn deleted_run_is_not_started_again(deployment: Deployment) {
    let run_id = deployment.fire_first_occurrence().await;
    deployment.wait_for_terminal_or_gone(&run_id).await;
    deployment.store.delete_run(&run_id).await.unwrap();
    deployment.wait_for_gone(&run_id).await;

    let mut restarted = deployment.replica(at(3, 0, 0));
    let outcome = deployment.evaluate(&mut restarted, at(3, 0, 0)).await;

    assert_eq!(outcome, Outcome::NotClaimed, "the instant stays consumed");
    assert_eq!(deployment.executions(), 1, "the flow must not run twice");
    assert_eq!(deployment.run_count().await, 0, "no second run is created");
}

/// IF-078: a replica that died between claiming and creating the run left no
/// run and no "run created" record, so the occurrence is still recovered.
async fn orphaned_claim_is_recovered(deployment: Deployment) {
    assert!(
        deployment
            .store
            .claim_schedule(NAME, KEY, CLAIM_TTL)
            .await
            .unwrap()
    );

    let mut restarted = deployment.replica(at(3, 0, 0));
    let outcome = deployment.evaluate(&mut restarted, at(3, 0, 0)).await;

    assert!(matches!(outcome, Outcome::Fired { .. }), "got {outcome:?}");
    deployment.wait_for_count(1).await;
    assert_eq!(deployment.run_count().await, 1);
}

/// A replica that died after creating the run but before recording it in the
/// claim: a peer converges on the run, repairs the claim, and the occurrence
/// then survives the loss of the record.
async fn converging_replica_repairs_the_claim(deployment: Deployment) {
    assert!(
        deployment
            .store
            .claim_schedule(NAME, KEY, CLAIM_TTL)
            .await
            .unwrap()
    );
    let run_id = {
        let _admission = PROCESS_ADMISSION_LOCK.lock().await;
        let executor = deployment.executor();
        let started = ScheduleExecutor::run(executor.as_ref(), NAME, KEY, &Deployment::schedule())
            .await
            .unwrap();
        let ScheduleRun::Started { run_id } = started else {
            panic!("the unrecorded run must start");
        };
        run_id
    };
    deployment.wait_for_count(1).await;
    deployment.wait_for_terminal_or_gone(&run_id).await;

    let mut peer = deployment.replica(at(2, 30, 0));
    let outcome = deployment.evaluate(&mut peer, at(2, 30, 0)).await;
    assert_eq!(outcome, Outcome::NotClaimed);

    deployment.store.delete_run(&run_id).await.unwrap();
    deployment.wait_for_gone(&run_id).await;
    let mut restarted = deployment.replica(at(3, 0, 0));
    let outcome = deployment.evaluate(&mut restarted, at(3, 0, 0)).await;

    assert_eq!(outcome, Outcome::NotClaimed);
    assert_eq!(deployment.executions(), 1, "the flow must not run twice");
}

mod json {
    use super::*;

    fn deployment() -> (Deployment, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ironflow::storage::json_store::JsonStateStore::new(
            dir.path(),
        ));
        (Deployment::new(store), dir)
    }

    #[tokio::test]
    async fn a_deleted_run_inside_grace_is_not_started_again() {
        let (deployment, _dir) = deployment();
        deleted_run_is_not_started_again(deployment).await;
    }

    #[tokio::test]
    async fn an_orphaned_claim_is_still_recovered() {
        let (deployment, _dir) = deployment();
        orphaned_claim_is_recovered(deployment).await;
    }

    #[tokio::test]
    async fn a_converging_replica_repairs_an_unrecorded_claim() {
        let (deployment, _dir) = deployment();
        converging_replica_repairs_the_claim(deployment).await;
    }
}

mod sqlite {
    use super::*;

    async fn deployment() -> (Deployment, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}/state.sqlite?mode=rwc", dir.path().display());
        let store = ironflow::storage::sql_store::SqlStateStore::new(&url)
            .await
            .unwrap();
        (Deployment::new(Arc::new(store)), dir)
    }

    #[tokio::test]
    async fn a_deleted_run_inside_grace_is_not_started_again() {
        let (deployment, _dir) = deployment().await;
        deleted_run_is_not_started_again(deployment).await;
    }

    #[tokio::test]
    async fn an_orphaned_claim_is_still_recovered() {
        let (deployment, _dir) = deployment().await;
        orphaned_claim_is_recovered(deployment).await;
    }

    #[tokio::test]
    async fn a_converging_replica_repairs_an_unrecorded_claim() {
        let (deployment, _dir) = deployment().await;
        converging_replica_repairs_the_claim(deployment).await;
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::postgres_support::PostgresStateTest;
    use super::*;

    /// Run one shared case against an isolated PostgreSQL table prefix.
    async fn run<F, Fut>(label: &str, case: F)
    where
        F: FnOnce(Deployment) -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let Some(fixture) = PostgresStateTest::from_env(label) else {
            return;
        };
        let store = Arc::new(fixture.state_store().await.unwrap());
        let result = tokio::spawn(case(Deployment::new(store))).await;
        fixture.cleanup().await.unwrap();
        result.unwrap();
    }

    #[tokio::test]
    async fn a_deleted_run_inside_grace_is_not_started_again() {
        run("pg_fence_delete", deleted_run_is_not_started_again).await;
    }

    #[tokio::test]
    async fn an_orphaned_claim_is_still_recovered() {
        run("pg_fence_orphan", orphaned_claim_is_recovered).await;
    }

    #[tokio::test]
    async fn a_converging_replica_repairs_an_unrecorded_claim() {
        run("pg_fence_repair", converging_replica_repairs_the_claim).await;
    }
}

#[cfg(feature = "redis")]
mod redis {
    use super::redis_support::RedisTest;
    use super::*;

    async fn deployment(label: &str, ttl: Option<u64>) -> Option<(Deployment, RedisTest)> {
        let fixture = RedisTest::connect(label).await?;
        let store = Arc::new(fixture.state_store(ttl).await);
        Some((Deployment::new(store), fixture))
    }

    /// The finding itself: `REDIS_TTL` far below the grace window. The record
    /// of the finished run expires on its own, and a replica that starts
    /// inside the grace window must still not run the instant again.
    #[tokio::test]
    async fn an_expired_record_inside_grace_is_not_started_again() {
        let Some((deployment, fixture)) = deployment("fence_expiry", Some(1)).await else {
            return;
        };

        let run_id = deployment.fire_first_occurrence().await;
        // Real key expiry, observed by polling rather than timed.
        deployment.wait_for_gone(&run_id).await;

        let mut restarted = deployment.replica(at(3, 0, 0));
        let outcome = deployment.evaluate(&mut restarted, at(3, 0, 0)).await;
        let executions = deployment.executions();
        fixture.cleanup().await;

        assert_eq!(outcome, Outcome::NotClaimed, "the instant stays consumed");
        assert_eq!(executions, 1, "the flow must not run twice");
    }

    #[tokio::test]
    async fn a_deleted_run_inside_grace_is_not_started_again() {
        let Some((deployment, fixture)) = deployment("fence_delete", None).await else {
            return;
        };
        let cleanup = RedisTest {
            url: fixture.url.clone(),
            prefix: fixture.prefix.clone(),
        };
        let result =
            tokio::spawn(async move { deleted_run_is_not_started_again(deployment).await }).await;
        cleanup.cleanup().await;
        result.unwrap();
    }

    #[tokio::test]
    async fn an_orphaned_claim_is_still_recovered() {
        let Some((deployment, fixture)) = deployment("fence_orphan", None).await else {
            return;
        };
        let result =
            tokio::spawn(async move { orphaned_claim_is_recovered(deployment).await }).await;
        fixture.cleanup().await;
        result.unwrap();
    }

    #[tokio::test]
    async fn a_converging_replica_repairs_an_unrecorded_claim() {
        let Some((deployment, fixture)) = deployment("fence_repair", None).await else {
            return;
        };
        let result =
            tokio::spawn(async move { converging_replica_repairs_the_claim(deployment).await })
                .await;
        fixture.cleanup().await;
        result.unwrap();
    }
}
