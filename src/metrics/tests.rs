use super::*;

struct StubScheduleExecutor;

#[async_trait::async_trait]
impl crate::scheduler::ScheduleExecutor for StubScheduleExecutor {
    async fn active_run(&self, _: &str) -> Option<String> {
        None
    }

    fn has_capacity(&self) -> bool {
        true
    }

    async fn run(
        &self,
        _: &str,
        _: &str,
        _: &crate::scheduler::config::ScheduleConfig,
    ) -> Result<crate::scheduler::ScheduleRun, String> {
        Ok(crate::scheduler::ScheduleRun::Started {
            run_id: "bounded-test-run".to_string(),
        })
    }
}

fn sample_value(encoded: &str, prefix: &str) -> f64 {
    encoded
        .lines()
        .find(|line| line.starts_with(prefix))
        .and_then(|line| line.rsplit_once(' '))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or_else(|| panic!("missing metric sample: {prefix}"))
}

#[test]
fn label_cardinality_is_fixed_and_inputs_are_not_labels() {
    let metrics = Metrics::new();
    let encoded = metrics.encode().unwrap();
    let samples = encoded
        .lines()
        .filter(|line| line.starts_with("ironflow_storage_failures_total{"))
        .count();
    assert_eq!(
        samples,
        (StorageOperation::STATE.len() + StorageOperation::EVENT.len()) * 5
    );

    for forbidden in [
        "run_id=",
        "flow=",
        "schedule=",
        "url=",
        "error=",
        "context=",
        "secret=",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "found forbidden label {forbidden}"
        );
    }
}

#[test]
fn concurrent_updates_are_lossless() {
    let metrics = Arc::new(Metrics::new());
    let mut workers = Vec::new();
    for _ in 0..8 {
        let metrics = metrics.clone();
        workers.push(std::thread::spawn(move || {
            for _ in 0..1_000 {
                metrics.admission(AdmissionResource::Run, AdmissionDecision::Accepted);
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }

    let encoded = metrics.encode().unwrap();
    assert_eq!(
        sample_value(
            &encoded,
            "ironflow_admission_decisions_total{resource=\"run\",decision=\"accepted\"}"
        ),
        8_000.0
    );
}

#[test]
fn registries_reset_with_the_process_instance() {
    let first = Metrics::new();
    first.admission(AdmissionResource::Run, AdmissionDecision::AtCapacity);
    let second = Metrics::new();

    assert_eq!(
        sample_value(
            &second.encode().unwrap(),
            "ironflow_admission_decisions_total{resource=\"run\",decision=\"at_capacity\"}"
        ),
        0.0
    );
}

#[tokio::test]
async fn workflow_execution_records_outcomes_durations_and_active_work() {
    let metrics = Arc::new(Metrics::new());
    let engine = crate::engine::executor::WorkflowEngine::new(
        Arc::new(crate::nodes::NodeRegistry::with_builtins()),
        Arc::new(crate::storage::null_store::NullStateStore::new()),
        None,
    )
    .with_metrics(Some(metrics.clone()));
    let flow = crate::engine::types::FlowDefinition {
        name: "metrics-test".to_string(),
        steps: vec![crate::engine::types::StepDefinition {
            name: "log".to_string(),
            node_type: "log".to_string(),
            config: serde_json::json!({"message": "hello"}),
            dependencies: Vec::new(),
            retry: crate::engine::types::RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    };

    engine
        .execute(&flow, crate::engine::types::Context::new())
        .await
        .unwrap();
    let encoded = metrics.encode().unwrap();

    assert_eq!(
        sample_value(&encoded, "ironflow_runs_total{outcome=\"success\"}"),
        1.0
    );
    assert_eq!(
        sample_value(
            &encoded,
            "ironflow_task_attempts_total{outcome=\"success\"}"
        ),
        1.0
    );
    assert_eq!(
        sample_value(&encoded, "ironflow_active_work{kind=\"run\"}"),
        0.0
    );
    assert_eq!(
        sample_value(&encoded, "ironflow_active_work{kind=\"task\"}"),
        0.0
    );
    assert_eq!(
        sample_value(
            &encoded,
            "ironflow_run_duration_seconds_count{outcome=\"success\"}"
        ),
        1.0
    );
    assert_eq!(
        sample_value(
            &encoded,
            "ironflow_task_attempt_duration_seconds_count{outcome=\"success\"}"
        ),
        1.0
    );
}

#[tokio::test]
async fn scheduler_evaluation_records_its_bounded_outcome() {
    use chrono::TimeZone as _;

    let start = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 1, 59, 0).unwrap();
    let due = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 2, 0, 0).unwrap();
    let schedules = std::collections::HashMap::from([(
        "secret-schedule-name".to_string(),
        crate::scheduler::config::ScheduleConfig::new(
            "secret-flow.lua",
            "0 2 * * *",
            Some("UTC"),
            None,
            crate::engine::types::Context::new(),
        )
        .unwrap(),
    )]);
    let metrics = Arc::new(Metrics::new());
    let mut scheduler = crate::scheduler::Scheduler::new(
        schedules,
        Arc::new(crate::storage::null_store::NullStateStore::new()),
        Arc::new(StubScheduleExecutor),
        start,
    )
    .with_metrics(Some(metrics.clone()));

    let decisions = scheduler.evaluate(due).await;
    assert_eq!(decisions.len(), 1);
    let encoded = metrics.encode().unwrap();
    assert!(encoded.contains("ironflow_scheduler_decisions_total{outcome=\"fired\"} 1"));
    assert!(!encoded.contains("secret-schedule-name"));
    assert!(!encoded.contains("secret-flow.lua"));
}
