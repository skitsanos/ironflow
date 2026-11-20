use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::{Arc, Metrics, sample_value};
use crate::engine::executor::WorkflowEngine;
use crate::engine::types::{Context, RunStatus};
use crate::lua::runtime::LuaRuntime;
use crate::nodes::{Node, NodeRegistry};
use crate::storage::{StateStore, null_store::NullStateStore};
use serde_json::json;

#[derive(Default)]
struct FailOnce(AtomicUsize);

#[async_trait::async_trait]
impl Node for FailOnce {
    fn node_type(&self) -> &str {
        "fail_once"
    }
    fn description(&self) -> &str {
        "Fail the first attempt"
    }
    async fn execute(&self, _: &serde_json::Value, _: &Context) -> anyhow::Result<Context> {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            anyhow::bail!("retry me");
        }
        Ok(Context::from([("repeat_done".into(), json!(true))]))
    }
}

async fn check_attempts(call: &str, detached: bool) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.lua"),
        r#"local flow = Flow.new("child")
           flow:step("retry", nodes.fail_once({})):retries(1, 0)
           return flow"#,
    )
    .unwrap();
    let mut registry = NodeRegistry::with_builtins();
    registry.register(Arc::new(FailOnce::default()));
    let base = Arc::new(registry.snapshot());
    crate::nodes::composition::register_nested(&mut registry, base);
    let parent = format!("local f = Flow.new('parent')\nf:step('call', {call})\nreturn f");
    let flow = LuaRuntime::load_flow_from_string(&parent, &registry).unwrap();
    let metrics = Arc::new(Metrics::new());
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), None)
        .with_metrics(Some(metrics.clone()));
    let ctx = Context::from([
        ("_flow_dir".into(), json!(dir.path())),
        (
            "calls".into(),
            json!([{"id":"1", "name":"probe", "arguments":{}}]),
        ),
    ]);
    let id = engine.execute(&flow, ctx).await.unwrap();
    assert_eq!(
        store.get_run_info(&id).await.unwrap().status,
        RunStatus::Success,
        "{call}"
    );
    if detached {
        // The parent can finish before the child records either attempt.
        tokio::time::timeout(Duration::from_secs(5), async {
            while metrics
                .task_attempts
                .get_or_create(&super::OutcomeLabels { outcome: "success" })
                .get()
                < 2
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("detached child did not report its attempt");
    }
    let encoded = metrics.encode().unwrap();
    for (outcome, count) in [("success", 2.0), ("failed", 1.0)] {
        for metric in ["task_attempts_total", "task_attempt_duration_seconds_count"] {
            let key = format!("ironflow_{metric}{{outcome=\"{outcome}\"}}");
            assert_eq!(sample_value(&encoded, &key), count, "{call}: {key}");
        }
    }
    assert_eq!(
        sample_value(&encoded, "ironflow_runs_total{outcome=\"success\"}"),
        1.0
    );
}

#[tokio::test]
async fn waiting_children_report_each_attempt_once() {
    for call in [
        r#"nodes.subworkflow({flow="child.lua"})"#,
        r#"nodes.parallel_subworkflows({flows={{flow="child.lua"}}})"#,
        r#"nodes.repeat_subworkflow({flow="child.lua",max_iterations=1})"#,
        r#"nodes.tool_dispatch({source_key="calls",tools={probe={flow="child.lua"}}})"#,
    ] {
        check_attempts(call, false).await;
    }
}

#[tokio::test]
async fn detached_child_reports_each_attempt_once() {
    check_attempts(r#"nodes.subworkflow({flow="child.lua",wait=false})"#, true).await;
}
