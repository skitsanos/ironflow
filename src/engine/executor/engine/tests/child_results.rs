use super::*;
use crate::engine::types::{RetryConfig, RunStatus, StepDefinition};
use crate::storage::{json_store::JsonStateStore, sql_store::SqlStateStore};

const BYTES: usize = 3 * 1024 * 1024;

fn flow(fail: bool) -> FlowDefinition {
    let mut steps = vec![StepDefinition {
        name: "produce".into(),
        node_type: "code".into(),
        config: serde_json::json!({"source": "return { payload = string.rep('x', 3 * 1024 * 1024), echoed = ctx._headers and ctx._headers.authorization, _headers = ctx._headers }"}),
        dependencies: Vec::new(),
        retry: RetryConfig::default(),
        timeout_s: None,
        route: None,
        on_error: None,
    }];
    if fail {
        steps.push(StepDefinition {
            name: "fail".into(),
            node_type: "code".into(),
            config: serde_json::json!({"source": "error('expected child failure')"}),
            dependencies: vec!["produce".into()],
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        });
    }
    FlowDefinition {
        name: "live-result".into(),
        steps,
    }
}

#[tokio::test]
async fn child_completion_is_redacted_and_independent_of_json_sqlite_and_null_history() {
    let directory = tempfile::tempdir().unwrap();
    let stores: Vec<Arc<dyn StateStore>> = vec![
        Arc::new(NullStateStore::new()),
        Arc::new(JsonStateStore::new(directory.path().join("json"))),
        Arc::new(
            SqlStateStore::new(&format!(
                "sqlite://{}?mode=rwc",
                directory.path().join("state.db").display()
            ))
            .await
            .unwrap(),
        ),
    ];
    for store in stores {
        let engine =
            WorkflowEngine::new(Arc::new(NodeRegistry::with_builtins()), store.clone(), None);
        for fail in [false, true] {
            let overlay = ExecutionOverlay::new(Context::from([(
                "_headers".into(),
                serde_json::json!({"authorization": "if116-secret-sentinel"}),
            )]));
            let id = Uuid::new_v4().to_string();
            let result = engine
                .start_new_with_run_id(&flow(fail), Context::new(), overlay, id.clone(), true)
                .await
                .unwrap()
                .unwrap()
                .wait_child_cancel_on_drop()
                .await
                .unwrap();
            let expected = if fail {
                RunStatus::Failed
            } else {
                RunStatus::Success
            };
            assert_eq!(result.status, expected);
            assert_eq!(result.flow_name, "live-result");
            let text = result.ctx["payload"].as_str().unwrap();
            assert_eq!(text.len(), BYTES);
            assert!(text.bytes().all(|byte| byte == b'x'));
            assert_eq!(result.ctx["echoed"], "[REDACTED]");
            assert!(!result.ctx.contains_key("_headers"));
            let info = store.get_run_info(&id).await.unwrap();
            assert_eq!(info.status, expected);
            assert!(info.finished.is_some());
            assert_eq!(info.ctx["payload"]["_truncated"], true);
            assert_eq!(
                info.tasks["produce"].output.as_ref().unwrap()["_truncated"],
                true
            );
            assert!(
                !serde_json::to_string(&info)
                    .unwrap()
                    .contains("if116-secret-sentinel")
            );
        }
    }
}

#[tokio::test]
async fn public_runs_do_not_retain_a_live_result_in_the_completion_channel() {
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(NodeRegistry::with_builtins()), store.clone(), None);
    let handle = engine.start(&flow(false), Context::new()).await.unwrap();
    let id = handle.id().to_string();
    let error = handle
        .wait_child_cancel_on_drop()
        .await
        .err()
        .expect("ordinary handle must not contain a live result");
    assert!(
        error
            .to_string()
            .contains("did not retain a live completion result")
    );
    let info = store.get_run_info(&id).await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx["payload"]["_truncated"], true);
}
