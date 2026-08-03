//! Persistence-boundary coverage for execution-only webhook headers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use http_body_util::BodyExt as _;
use ironflow::api::{AppState, WebhookConfig};
use ironflow::engine::RunEventType;
use ironflow::engine::types::{Context, RunInfo, RunStatus, RunSummary, TaskState};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::{EventStore as _, MemoryEventStore};
use ironflow::storage::null_store::NullStateStore;
use ironflow::storage::{RunListQuery, RunSummaryPage, StateStore, StorageError, StorageResult};
use tower::ServiceExt as _;

const SENTINEL: &str = "if011-persistence-secret-sentinel-8c76f2";

#[derive(Default)]
struct RecordingStateStore {
    inner: NullStateStore,
    serialized_writes: Mutex<Vec<String>>,
}

impl RecordingStateStore {
    fn record<T: serde::Serialize>(&self, operation: &str, arguments: &T) -> StorageResult<()> {
        let serialized = serde_json::to_string(&(operation, arguments)).map_err(|error| {
            StorageError::backend("Failed to record state-store test call", error)
        })?;
        self.serialized_writes
            .lock()
            .map_err(|error| StorageError::backend("Failed to lock state-store recorder", error))?
            .push(serialized);
        Ok(())
    }

    fn serialized_writes(&self) -> Vec<String> {
        self.serialized_writes.lock().unwrap().clone()
    }
}

#[async_trait]
impl StateStore for RecordingStateStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()> {
        self.record("init_run", &(run_id, flow_name, ctx))?;
        self.inner.init_run(run_id, flow_name, ctx).await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        self.inner.set_run_status(run_id, status).await
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        self.record("upsert_task", &(run_id, task))?;
        self.inner.upsert_task(run_id, task).await
    }

    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context> {
        self.inner.get_ctx(run_id).await
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        self.record("update_ctx", &(run_id, ctx))?;
        self.inner.update_ctx(run_id, ctx).await
    }

    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo> {
        self.inner.get_run_info(run_id).await
    }

    async fn list_runs(&self, status: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        self.inner.list_runs(status).await
    }

    async fn list_run_summaries(
        &self,
        status: Option<RunStatus>,
    ) -> StorageResult<Vec<RunSummary>> {
        self.inner.list_run_summaries(status).await
    }

    async fn list_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage> {
        self.inner.list_run_summaries_page(query).await
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<()> {
        self.inner.delete_run(run_id).await
    }
}

#[tokio::test]
async fn forwarded_header_never_crosses_state_or_event_boundaries() {
    let flow_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        flow_dir.path().join("signed.lua"),
        format!(
            r#"
            local flow = Flow.new("webhook_persistence_boundaries")

            flow:step("observe", function(ctx)
                local signature = ctx._headers["x-business-signature"]
                return {{
                    header_visible = signature == "{SENTINEL}",
                    copied_signature = signature
                }}
            end)

            flow:step("fail_with_header", function(ctx)
                error("provider rejected signature " .. ctx._headers["x-business-signature"])
            end):depends_on("observe"):on_error("recover")

            flow:step("shell_fail", nodes.shell_command({{
                cmd = "sh",
                args = {{ "-c", "printf '%s' \"$IF022_SECRET\"; printf '%s' \"$IF022_SECRET\" >&2; exit 7" }},
                env = {{
                    IF022_SECRET = '${{ctx._headers["x-business-signature"]}}'
                }},
                output_key = "shell_secret"
            }})):depends_on("observe"):on_error("recover_shell")

            flow:step("recover", function(ctx)
                local signature = ctx._headers["x-business-signature"]
                return {{
                    recovery_header_visible = signature == "{SENTINEL}",
                    recovery_copy = signature,
                    recovered_error = ctx._error_message
                }}
            end):depends_on("observe")

            flow:step("recover_shell", function(ctx)
                return {{
                    shell_recovery_direct = ctx.shell_secret_stderr,
                    shell_recovery_exact = ctx._error_output.shell_secret_stdout
                }}
            end):depends_on("observe")

            return flow
            "#,
        ),
    )
    .unwrap();

    let store = Arc::new(RecordingStateStore::default());
    let events = Arc::new(MemoryEventStore::new());
    let webhook = WebhookConfig::new("signed.lua", ["x-business-signature".to_string()]).unwrap();
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: store.clone(),
        event_store: events.clone(),
        flows_dir: Some(flow_dir.path().to_path_buf()),
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: HashMap::from([("signed".to_string(), webhook)]),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
    });
    let app = Router::new()
        .route(
            "/webhooks/{name}",
            post(ironflow::api::handlers::run_webhook),
        )
        .with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/signed")
                .header("content-type", "application/json")
                .header("x-business-signature", SENTINEL)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let response_body = response.into_body().collect().await.unwrap().to_bytes();
    let response_json: serde_json::Value = serde_json::from_slice(&response_body).unwrap();
    assert_eq!(response_json["status"], "success");
    assert!(!String::from_utf8_lossy(&response_body).contains(SENTINEL));

    let run_id = response_json["run_id"].as_str().unwrap();
    let run_info = store.get_run_info(run_id).await.unwrap();
    assert_eq!(run_info.ctx["header_visible"], true);
    assert_eq!(run_info.ctx["recovery_header_visible"], true);
    assert_eq!(run_info.ctx["copied_signature"], "[REDACTED]");
    assert_eq!(run_info.ctx["recovery_copy"], "[REDACTED]");
    assert_eq!(run_info.ctx["shell_secret_stdout"], "[REDACTED]");
    assert_eq!(run_info.ctx["shell_secret_stderr"], "[REDACTED]");
    assert_eq!(run_info.ctx["shell_recovery_direct"], "[REDACTED]");
    assert_eq!(run_info.ctx["shell_recovery_exact"], "[REDACTED]");
    let shell_output = run_info.tasks["shell_fail"].output.as_ref().unwrap();
    assert_eq!(shell_output["shell_secret_stdout"], "[REDACTED]");
    assert_eq!(shell_output["shell_secret_stderr"], "[REDACTED]");
    assert!(
        run_info.ctx["recovered_error"]
            .as_str()
            .unwrap()
            .contains("[REDACTED]")
    );

    let writes = store.serialized_writes();
    assert!(writes.iter().any(|write| write.contains("init_run")));
    assert!(writes.iter().any(|write| write.contains("upsert_task")));
    assert!(writes.iter().any(|write| write.contains("update_ctx")));
    let serialized_writes = writes.join("\n");
    assert!(!serialized_writes.contains(SENTINEL));
    assert!(!serialized_writes.contains("\"_headers\""));

    let events = events.list_since(run_id, None, 100).await.unwrap();
    assert!(events.iter().any(|event| {
        event.event_type == RunEventType::TaskFailed
            && event
                .error
                .as_deref()
                .is_some_and(|error| error.contains("[REDACTED]"))
    }));
    let serialized_events = serde_json::to_string(&events).unwrap();
    assert!(!serialized_events.contains(SENTINEL));
    assert!(!serialized_events.contains("\"_headers\""));
}
