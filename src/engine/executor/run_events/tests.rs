//! IF-214: admission to the event stream is retried, counted and surfaced.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::storage::StorageError;
use crate::storage::event_store::MemoryEventStore;
use crate::storage::null_store::NullStateStore;

enum Fault {
    Error,
    Hang,
}

/// Fails or hangs the first `failing` admissions, then admits.
struct FaultyAdmission {
    inner: MemoryEventStore,
    fault: Fault,
    failing: usize,
    attempts: AtomicUsize,
}

impl FaultyAdmission {
    fn new(fault: Fault, failing: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: MemoryEventStore::new(),
            fault,
            failing,
            attempts: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl EventStore for FaultyAdmission {
    async fn publish(&self, event: RunEvent) -> StorageResult<()> {
        self.inner.publish(event).await
    }

    async fn publish_as(&self, event: RunEvent, incarnation: &str) -> StorageResult<()> {
        self.inner.publish_as(event, incarnation).await
    }

    async fn begin_incarnation(&self, run_id: &str, incarnation: &str) -> StorageResult<()> {
        if self.attempts.fetch_add(1, Ordering::SeqCst) < self.failing {
            return match self.fault {
                Fault::Error => Err(StorageError::backend(
                    "Injected admission failure",
                    "if214-secret-diagnostic",
                )),
                Fault::Hang => std::future::pending().await,
            };
        }
        self.inner.begin_incarnation(run_id, incarnation).await
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<usize> {
        self.inner.delete_run(run_id).await
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> StorageResult<Vec<RunEvent>> {
        self.inner.list_since(run_id, after, limit).await
    }
}

fn count(metrics: &Metrics, outcome: &str) -> String {
    let encoded = metrics.encode().unwrap();
    let series = format!("ironflow_event_admissions_total{{outcome=\"{outcome}\"}} ");
    encoded
        .lines()
        .find_map(|line| line.strip_prefix(series.as_str()))
        .unwrap_or_else(|| panic!("no series for {outcome}"))
        .to_string()
}

fn counts(metrics: &Metrics) -> [String; 3] {
    ["admitted", "admitted_after_retry", "failed"].map(|outcome| count(metrics, outcome))
}

async fn begin_with(store: &Arc<FaultyAdmission>, metrics: &Arc<Metrics>) -> Admission {
    let events: Arc<dyn EventStore> = store.clone();
    begin(&events, "run", "incarnation", Some(metrics)).await
}

#[tokio::test(start_paused = true)]
async fn first_attempt_admission_is_counted_without_a_warning() {
    let metrics = Arc::new(Metrics::new());
    let store = FaultyAdmission::new(Fault::Error, 0);

    let admission = begin_with(&store, &metrics).await;

    assert_eq!(admission.warning, None);
    assert_eq!(store.attempts.load(Ordering::SeqCst), 1);
    assert_eq!(counts(&metrics), ["1", "0", "0"]);
}

#[tokio::test(start_paused = true)]
async fn failed_and_timed_out_attempts_are_retried() {
    for fault in [Fault::Error, Fault::Hang] {
        let metrics = Arc::new(Metrics::new());
        let store = FaultyAdmission::new(fault, 2);

        let admission = begin_with(&store, &metrics).await;

        assert_eq!(admission.warning, None);
        assert_eq!(store.attempts.load(Ordering::SeqCst), 3);
        assert_eq!(counts(&metrics), ["0", "1", "0"]);
        assert_eq!(
            store.inner.admitted_incarnation("run").await.unwrap(),
            Some("incarnation".to_string())
        );
    }
}

#[tokio::test(start_paused = true)]
async fn exhausted_admission_is_counted_and_returned_as_a_warning() {
    for (fault, cause) in [(Fault::Error, "store error"), (Fault::Hang, "timed out")] {
        let metrics = Arc::new(Metrics::new());
        let store = FaultyAdmission::new(fault, usize::MAX);
        let started = tokio::time::Instant::now();

        let admission = begin_with(&store, &metrics).await;

        assert_eq!(store.attempts.load(Ordering::SeqCst), 3);
        assert_eq!(counts(&metrics), ["0", "0", "1"]);
        let warning = admission.warning.unwrap();
        assert!(warning.contains("3 attempts"), "{warning}");
        assert!(warning.contains(cause), "{warning}");
        // The warning is stored on the run: it names no error text.
        assert!(!warning.contains("if214-secret"), "{warning}");
        // Three bounded attempts and two pauses, also when the store hangs.
        assert!(started.elapsed() <= Duration::from_millis(3_250));
    }
}

#[tokio::test]
async fn warning_is_recorded_in_the_context_and_on_the_stored_run() {
    let store = NullStateStore::new();
    store
        .init_run("run", "flow", &Context::new())
        .await
        .unwrap();
    let mut ctx = Context::from([("kept".to_string(), serde_json::json!(1))]);

    record_warning(&store, "run", "owner", &mut ctx, "no events".to_string()).await;

    let expected = serde_json::json!("no events");
    assert_eq!(ctx.get(EVENT_STREAM_WARNING_KEY), Some(&expected));
    assert_eq!(ctx.get("kept"), Some(&serde_json::json!(1)));
    let stored = store.get_ctx("run").await.unwrap();
    assert_eq!(stored.get(EVENT_STREAM_WARNING_KEY), Some(&expected));
}
