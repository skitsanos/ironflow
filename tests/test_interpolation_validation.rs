//! End-to-end validation coverage for context interpolation in workflow configs.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, RunInfo, RunStatus, StepDefinition, TaskState,
};
use ironflow::lua::interpolate::{interpolate_ctx, interpolate_value};
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::null_store::NullStateStore;
use ironflow::storage::{RunListQuery, RunSummaryPage, StateStore, StorageResult};

fn step_with_config(name: &str, config: serde_json::Value) -> StepDefinition {
    StepDefinition {
        name: name.to_string(),
        node_type: "log".to_string(),
        config,
        dependencies: Vec::new(),
        retry: Default::default(),
        timeout_s: None,
        route: None,
        on_error: None,
    }
}

fn flow_with_config(name: &str, config: serde_json::Value) -> FlowDefinition {
    FlowDefinition {
        name: "interpolation_validation".to_string(),
        steps: vec![step_with_config(name, config)],
    }
}

#[test]
fn validation_reports_step_and_recursive_config_path() {
    let flow = flow_with_config(
        "render",
        serde_json::json!({
            "payload": {
                "messages": [
                    {"text": "Hello ${ctx.user.name or 'unknown'}"}
                ]
            }
        }),
    );

    let errors = flow.validate_dag().join("\n");
    assert!(errors.contains("Step 'render'"), "{errors}");
    assert!(
        errors.contains("config.payload.messages[0].text"),
        "{errors}"
    );
}

#[test]
fn validation_ignores_foreign_shell_expansions_and_currency_prefixes() {
    let flow = flow_with_config(
        "shell",
        serde_json::json!({
            "home": "${HOME}",
            "temporary": "${TMPDIR:-/tmp}",
            "price": "$${ctx.amount}",
            "escaped": r"\${ctx.amount}",
            "first": "${ctx.items[0].name}",
            "quoted": "${ctx[\"key.with.dots\"]}",
            "nested": ["${SHELL}", {"fallback": "${CACHE_DIR:-/var/tmp}"}]
        }),
    );

    let errors = flow.validate_dag();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn interpolation_supports_zero_based_arrays_quoted_keys_and_escaping() {
    let ctx = Context::from([
        (
            "items".to_string(),
            serde_json::json!([{"name": "first"}, {"name": "second"}]),
        ),
        ("key.with.dots".to_string(), serde_json::json!("quoted")),
        ("amount".to_string(), serde_json::json!(42)),
        ("nothing".to_string(), serde_json::Value::Null),
    ]);
    let template = concat!(
        "${ctx.items[0].name}|${ctx.items[1].name}|",
        "${ctx[\"key.with.dots\"]}|$${ctx.amount}|",
        r"\${ctx.amount}|${ctx.missing}|${ctx.nothing}",
    );

    assert_eq!(
        interpolate_ctx(template, &ctx),
        "first|second|quoted|$42|${ctx.amount}||"
    );
}

#[test]
fn recursive_interpolation_visits_values_but_not_object_keys() {
    let ctx = Context::from([
        ("name".to_string(), serde_json::json!("Ada")),
        ("items".to_string(), serde_json::json!([{"id": 7}])),
    ]);
    let config = serde_json::json!({
        "${ctx.name}": "object keys stay literal",
        "payload": {
            "message": "Hello ${ctx.name}",
            "items": ["id=${ctx.items[0].id}", 42, true, null]
        }
    });

    assert_eq!(
        interpolate_value(&config, &ctx),
        serde_json::json!({
            "${ctx.name}": "object keys stay literal",
            "payload": {
                "message": "Hello Ada",
                "items": ["id=7", 42, true, null]
            }
        })
    );
}

#[test]
fn validation_checks_the_evaluated_lua_configuration() {
    let registry = NodeRegistry::with_builtins();
    let source = r#"
        local flow = Flow.new("invalid_interpolation")
        local result_key = "results"

        flow:step("render", nodes.log({
            message = "valid",
            payload = {
                [result_key] = {
                    { text = "${ctx.profile.display_name or 'anonymous'}" }
                }
            }
        }))

        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &registry).unwrap();
    let errors = flow.validate_dag().join("\n");
    assert!(errors.contains("Step 'render'"), "{errors}");
    assert!(
        errors.contains("config.payload.results[0].text"),
        "{errors}"
    );
}

#[test]
fn lua_runtime_escape_is_validated_after_source_evaluation() {
    let registry = NodeRegistry::with_builtins();
    let source = r#"
        local flow = Flow.new("escaped_interpolation")
        flow:step("show", nodes.log({
            message = "\\${ctx.amount}",
            shell_value = "${TMPDIR:-/tmp}"
        }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &registry).unwrap();
    assert_eq!(flow.steps[0].config["message"], r"\${ctx.amount}");
    let errors = flow.validate_dag();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[derive(Default)]
struct InitCountingStore {
    inner: NullStateStore,
    init_run_calls: AtomicUsize,
}

impl InitCountingStore {
    fn init_run_calls(&self) -> usize {
        self.init_run_calls.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl StateStore for InitCountingStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()> {
        self.init_run_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.init_run(run_id, flow_name, ctx).await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        self.inner.set_run_status(run_id, status).await
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        self.inner.upsert_task(run_id, task).await
    }

    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context> {
        self.inner.get_ctx(run_id).await
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        self.inner.update_ctx(run_id, ctx).await
    }

    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo> {
        self.inner.get_run_info(run_id).await
    }

    async fn list_runs(&self, status: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        self.inner.list_runs(status).await
    }

    async fn list_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage> {
        self.inner.list_run_summaries_page(query).await
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<()> {
        self.inner.delete_run(run_id).await
    }
}

#[tokio::test]
async fn engine_rejects_invalid_interpolation_before_initializing_state() {
    let store = Arc::new(InitCountingStore::default());
    let engine = WorkflowEngine::new(Arc::new(NodeRegistry::with_builtins()), store.clone(), None);
    let flow = flow_with_config(
        "request",
        serde_json::json!({
            "headers": {
                "authorization": "Bearer ${ctx.token or env('API_TOKEN')}"
            }
        }),
    );

    let error = match engine.start(&flow, Context::new()).await {
        Ok(_) => panic!("invalid interpolation unexpectedly started a run"),
        Err(error) => error,
    };

    let message = error.to_string();
    assert!(message.contains("Invalid flow"), "{message}");
    assert!(
        message.contains("config.headers.authorization"),
        "{message}"
    );
    assert_eq!(store.init_run_calls(), 0);
}
