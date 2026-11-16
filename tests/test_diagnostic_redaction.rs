//! Task-error scrubbing at the engine boundary (IF-177).
//!
//! The scrubber used to run a quadratic scan inline on the task future, so a
//! large error of sensitive words stalled a worker for minutes, and it missed
//! compound keys, plural keys and `Bearer`/`Basic` values after `=`.

use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};
use async_trait::async_trait;
use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, NodeOutput, RetryConfig, RunInfo, RunStatus, StepDefinition,
};
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::nodes::{Node, NodeRegistry};
use ironflow::storage::StateStore;
use ironflow::storage::event_store::{EventStore, MemoryEventStore};
use ironflow::storage::null_store::NullStateStore;

/// Generous for an unoptimized build under load. The quadratic scan needed
/// 334 s for a quarter of this input on an optimized build.
const BUDGET: Duration = Duration::from_secs(30);

struct FailWith(String);

#[async_trait]
impl Node for FailWith {
    fn node_type(&self) -> &str {
        "test_diagnostic"
    }

    fn description(&self) -> &str {
        "Fail with a fixed message"
    }

    async fn execute(&self, _config: &serde_json::Value, _ctx: &Context) -> Result<NodeOutput> {
        bail!("{}", self.0)
    }
}

async fn fail_once(message: String) -> (RunInfo, Vec<RunEvent>) {
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(FailWith(message)));
    let store = Arc::new(NullStateStore::new());
    let events = Arc::new(MemoryEventStore::new());
    let engine =
        WorkflowEngine::new_with_events(Arc::new(registry), store.clone(), events.clone(), None);
    let flow = FlowDefinition {
        name: "diagnostic_redaction".to_string(),
        steps: vec![StepDefinition {
            name: "work".to_string(),
            node_type: "test_diagnostic".to_string(),
            config: serde_json::json!({}),
            dependencies: vec![],
            retry: RetryConfig {
                max_retries: 0,
                backoff_s: 0.0,
            },
            timeout_s: Some(5.0),
            route: None,
            on_error: None,
        }],
    };

    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();
    let events = events.list_since(&run_id, None, 100).await.unwrap();
    (info, events)
}

/// Run the flow on its own thread and runtime, so a scan that never yields
/// fails the test instead of hanging it.
#[track_caller]
fn fail_once_within_budget(message: String) -> (RunInfo, Vec<RunEvent>) {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _ = sender.send(runtime.block_on(fail_once(message)));
    });
    receiver
        .recv_timeout(BUDGET)
        .unwrap_or_else(|_| panic!("the failed step did not settle within {BUDGET:?}"))
}

fn failure_texts(info: &RunInfo, events: &[RunEvent]) -> Vec<String> {
    let mut texts = vec![info.tasks["work"].error.clone().expect("task error")];
    texts.extend(events.iter().filter_map(|event| event.error.clone()));
    texts
}

#[test]
fn every_reported_credential_shape_is_removed_from_the_task_record_and_events() {
    let message = concat!(
        "upstream rejected: password=hunter1 credentials=hunter2 db_password=hunter3 ",
        "PGPASSWORD=hunter4 aws_secret_access_key=hunter5 X-API-Key: hunter6 ",
        "Authorization=Bearer hunter7 Proxy-Authorization: Basic hunter8"
    );

    let (info, events) = fail_once_within_budget(message.to_string());

    assert_eq!(info.status, RunStatus::Failed);
    let texts = failure_texts(&info, &events);
    assert!(
        events
            .iter()
            .any(|event| event.event_type == RunEventType::TaskFailed && event.error.is_some()),
        "the failed attempt publishes its error"
    );
    for text in &texts {
        assert!(!text.contains("hunter"), "{text}");
    }
    assert!(
        texts[0].starts_with("upstream rejected: password=[REDACTED] credentials=[REDACTED] "),
        "{}",
        texts[0]
    );
}

#[test]
fn text_that_only_resembles_a_credential_is_kept_in_the_task_record() {
    let message = "quota exceeded: max_tokens: 4096, token_type=bearer, password_policy: strict";

    let (info, _) = fail_once_within_budget(message.to_string());

    assert_eq!(info.tasks["work"].error.as_deref(), Some(message));
}

#[test]
fn a_mebibyte_of_sensitive_words_settles_within_budget_and_is_stored_bounded() {
    let message = "token ".repeat(1024 * 1024 / 6 + 1);
    assert!(message.len() > 1024 * 1024);

    let (info, events) = fail_once_within_budget(message);

    assert_eq!(info.status, RunStatus::Failed);
    for text in failure_texts(&info, &events) {
        assert!(text.len() < 80 * 1024, "stored {} bytes", text.len());
        assert!(
            text.ends_with("...[truncated]"),
            "{}",
            &text[text.len() - 32..]
        );
    }
}
