//! IF-219 (c): redaction is record-only.
//!
//! A forwarded header value is withheld from everything IronFlow records or
//! returns, but the data one step hands to the next is never rewritten. Until
//! this change a sender who controlled a forwarded header could blank any
//! string of eight or more bytes in a later step's output by sending it.

use ironflow::engine::types::TaskStatus;

use crate::test_webhook_header_redaction::run_webhook;

const REDACTED: &str = "[REDACTED]";
/// A made-up header value that is also ordinary business text in the flow.
const LABEL: &str = "acme-production";

#[tokio::test]
async fn a_forwarded_header_does_not_blank_text_a_later_step_reads() {
    let run = run_webhook(
        r#"
        local flow = Flow.new("if219_live_strings")
        flow:step("build", function(ctx)
            return {
                region = "acme-production-eu",
                label_copy = ctx._headers["x-tenant-label"],
                nested = { ["acme-production"] = "by key", list = { "acme-production" } }
            }
        end)
        flow:step("consume", function(ctx)
            return {
                region_matches = ctx.region == "acme-production-eu",
                region_bytes = #ctx.region,
                copy_bytes = #ctx.label_copy,
                key_survived = ctx.nested["acme-production"] == "by key",
                item_bytes = #ctx.nested.list[1]
            }
        end):depends_on("build")
        return flow
        "#,
        ("x-tenant-label", LABEL),
        "{}",
    )
    .await;

    assert_eq!(run.status, "success");
    // The next step read what the first one returned.
    assert_eq!(run.info.ctx["region_matches"], true);
    assert_eq!(run.info.ctx["region_bytes"], 18);
    assert_eq!(run.info.ctx["copy_bytes"], 15);
    assert_eq!(run.info.ctx["key_survived"], true);
    assert_eq!(run.info.ctx["item_bytes"], 15);
    // Every record of it is still redacted (IF-011).
    let built = run.info.tasks["build"].output.as_ref().unwrap();
    assert_eq!(built["region"], format!("{REDACTED}-eu"));
    assert_eq!(built["label_copy"], REDACTED);
    assert_eq!(built["nested"]["list"][0], REDACTED);
    assert!(built["nested"].get(LABEL).is_none());
    assert_eq!(run.info.ctx["region"], format!("{REDACTED}-eu"));
    assert_eq!(run.info.ctx["label_copy"], REDACTED);
    assert!(!run.durable_text.contains(LABEL));
    assert!(!run.durable_text.contains("\"_headers\""));
}

#[tokio::test]
async fn a_forwarded_header_cannot_flip_a_branch_that_compares_a_string() {
    let run = run_webhook(
        r#"
        local flow = Flow.new("if219_string_branch")
        flow:step("classify", function(ctx)
            return { tier = "acme-production" }
        end)
        flow:step_if('ctx.tier == "acme-production"', "bill", function(ctx)
            return { billed = true }
        end):depends_on("classify")
        return flow
        "#,
        ("x-tenant-label", LABEL),
        "{}",
    )
    .await;

    assert_eq!(run.status, "success");
    assert_eq!(run.info.tasks["bill"].status, TaskStatus::Success);
    assert_eq!(run.info.ctx["billed"], true);
    assert_eq!(run.info.ctx["tier"], REDACTED);
    assert!(!run.durable_text.contains(LABEL));
}

#[tokio::test]
async fn a_failed_step_keeps_its_message_scrubbed_for_records_and_handlers() {
    let run = run_webhook(
        r#"
        local flow = Flow.new("if219_error_text")
        flow:step("fail", function(ctx)
            error("upstream rejected " .. ctx._headers["x-tenant-label"])
        end):on_error("recover")
        flow:step("recover", function(ctx)
            return {
                saw_marker = string.find(ctx._error_message, "[REDACTED]", 1, true) ~= nil,
                saw_value = string.find(ctx._error_message, "acme-production", 1, true) ~= nil
            }
        end)
        return flow
        "#,
        ("x-tenant-label", LABEL),
        "{}",
    )
    .await;

    // A diagnostic is a record: the handler receives the scrubbed text that
    // is stored and published, never the raw one.
    assert_eq!(run.info.ctx["saw_marker"], true);
    assert_eq!(run.info.ctx["saw_value"], false);
    let error = run.info.tasks["fail"].error.as_deref().unwrap();
    assert!(error.contains(REDACTED), "{error}");
    assert!(!run.durable_text.contains(LABEL));
}

mod boundaries {
    //! One run that carries the header value through step output, a child
    //! run and two failures, then every place IronFlow records or returns
    //! it, checked one by one.

    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use http_body_util::BodyExt;
    use ironflow::api::{AppState, WebhookConfig};
    use ironflow::nodes::NodeRegistry;
    use ironflow::storage::StateStore as _;
    use ironflow::storage::event_store::{EventStore as _, MemoryEventStore};
    use ironflow::storage::json_store::JsonStateStore;
    use tower::ServiceExt;

    use super::{LABEL, REDACTED};
    use crate::webhook_support::PROCESS_ADMISSION_LOCK;

    const PARENT: &str = r#"
        local flow = Flow.new("if219_parent")
        flow:step("copy", function(ctx)
            return { label_copy = ctx._headers["x-tenant-label"], region = "acme-production-eu" }
        end)
        flow:step("child", nodes.subworkflow({
            flow = "child.lua",
            output_key = "child_out"
        })):depends_on("copy")
        flow:step("after", function(ctx)
            return {
                parent_read_child_copy = ctx.child_out.echoed == ctx._headers["x-tenant-label"],
                child_read_region = ctx.child_out.region_matches
            }
        end):depends_on("child")
        flow:step("boom", function(ctx)
            error("parent gave up on tenant " .. ctx.label_copy)
        end):depends_on("after")
        return flow
    "#;

    const CHILD: &str = r#"
        local flow = Flow.new("if219_child")
        flow:step("echo", function(ctx)
            return { echoed = ctx.label_copy, region_matches = ctx.region == "acme-production-eu" }
        end)
        flow:step("fail", function(ctx)
            error("child refused tenant " .. ctx.label_copy)
        end):depends_on("echo"):on_error("recover")
        flow:step("recover", function(ctx)
            return { recovered_error = ctx._error_message }
        end)
        return flow
    "#;

    async fn body_text(app: &Router, request: Request<Body>) -> String {
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn get_request(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn files_under(directory: &std::path::Path, files: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path, files);
            } else {
                let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
                files.push((path.display().to_string(), text));
            }
        }
    }

    #[track_caller]
    fn assert_withheld(boundary: &str, text: &str) {
        assert!(!text.is_empty(), "{boundary} produced nothing to check");
        assert!(!text.contains(LABEL), "{boundary} holds the header value");
        assert!(
            !text.contains("\"_headers\""),
            "{boundary} holds the overlay key"
        );
    }

    #[tokio::test]
    async fn every_recording_boundary_withholds_a_value_that_flowed_through_live_data() {
        let _admission = PROCESS_ADMISSION_LOCK.lock().await;
        let flows = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        std::fs::write(flows.path().join("parent.lua"), PARENT).unwrap();
        std::fs::write(flows.path().join("child.lua"), CHILD).unwrap();
        let store = Arc::new(JsonStateStore::new(store_dir.path()));
        let events = Arc::new(MemoryEventStore::new());
        let webhook = WebhookConfig::new("parent.lua", ["x-tenant-label".to_string()]).unwrap();
        let state = Arc::new(AppState {
            registry: Arc::new(NodeRegistry::with_builtins()),
            store: store.clone(),
            event_store: events.clone(),
            flows_dir: Some(flows.path().to_path_buf()),
            max_concurrent_tasks: None,
            listing_policy: ironflow::util::listing::ListingPolicy::default(),
            webhooks: HashMap::from([("hook".to_string(), webhook)]),
            allow_adhoc_flows: true,
            lifecycle: ironflow::api::ServiceLifecycle::default(),
            metrics: None,
        });
        let app = Router::new()
            .route(
                "/webhooks/{name}",
                post(ironflow::api::handlers::run_webhook),
            )
            .route("/runs", get(ironflow::api::handlers::list_runs))
            .route("/runs/{id}", get(ironflow::api::handlers::get_run))
            .route(
                "/runs/{id}/events",
                get(ironflow::api::handlers::run_events),
            )
            .with_state(state);

        let request = Request::builder()
            .method("POST")
            .uri("/webhooks/hook")
            .header("content-type", "application/json")
            .header("x-tenant-label", LABEL)
            .body(Body::from("{}"))
            .unwrap();
        let response = body_text(&app, request).await;
        let response_json: serde_json::Value = serde_json::from_str(&response).unwrap();
        let run_id = response_json["run_id"].as_str().unwrap();

        // The value did flow through live data, in the parent and the child.
        let parent = store.get_run_info(run_id).await.unwrap();
        assert_eq!(response_json["status"], "failed");
        assert_eq!(parent.ctx["parent_read_child_copy"], true);
        assert_eq!(parent.ctx["child_read_region"], true);
        // A child run keeps no record of its own: what it produced is
        // recorded as the output of the parent step that ran it.
        let runs = store.list_runs(None).await.unwrap();
        assert_eq!(runs.len(), 1);

        // 1. The webhook response.
        assert_withheld("webhook response", &response);
        // 2. The stored run record: context, task output and task errors,
        //    including everything the child returned.
        for run in &runs {
            let record = serde_json::to_string(run).unwrap();
            assert_withheld(&format!("stored run '{}'", run.flow_name), &record);
            assert!(record.contains(REDACTED), "{record}");
        }
        assert_eq!(parent.ctx["label_copy"], REDACTED);
        assert_eq!(parent.ctx["region"], format!("{REDACTED}-eu"));
        assert_eq!(parent.ctx["child_out"]["echoed"], REDACTED);
        let boom = parent.tasks["boom"].error.as_deref().unwrap();
        assert!(boom.contains(&format!("tenant {REDACTED}")), "{boom}");
        let recovered = parent.ctx["child_out"]["recovered_error"].as_str().unwrap();
        assert!(
            recovered.contains(&format!("tenant {REDACTED}")),
            "{recovered}"
        );
        // 3. Every file the state store wrote.
        let mut files = Vec::new();
        files_under(store_dir.path(), &mut files);
        assert!(files.len() >= 2);
        // Lock files are empty.
        for (path, text) in files.iter().filter(|(_, text)| !text.is_empty()) {
            assert_withheld(&format!("state file {path}"), text);
        }
        // 4. Stored events, 5. the SSE stream, 6. the run detail and
        //    7. the run list.
        for run in &runs {
            let stored = events.list_since(&run.id, None, 1000).await.unwrap();
            assert_withheld("event store", &serde_json::to_string(&stored).unwrap());
            let stream = body_text(&app, get_request(&format!("/runs/{}/events", run.id))).await;
            assert_withheld("SSE stream", &stream);
            assert!(stream.contains("task_failed"), "{stream}");
            let detail = body_text(&app, get_request(&format!("/runs/{}", run.id))).await;
            assert_withheld("GET /runs/{id}", &detail);
        }
        assert_withheld("GET /runs", &body_text(&app, get_request("/runs")).await);
    }
}

/// `examples/15-webhooks/forwarded_header.lua`, run as its header documents.
#[tokio::test]
async fn the_forwarded_header_example_reads_its_copy_while_records_withhold_it() {
    const TOKEN: &str = "tenant-7f3a9c21;region=eu";
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/15-webhooks/forwarded_header.lua"),
    )
    .unwrap();

    let run = run_webhook(&source, ("x-tenant-token", TOKEN), r#"{"amount": 250}"#).await;

    assert_eq!(run.status, "success");
    assert_eq!(run.info.ctx["copy_matches_header"], true);
    assert_eq!(run.info.ctx["tenant_token_present"], true);
    assert_eq!(run.info.ctx["region"], "eu");
    assert_eq!(run.info.tasks["needs_approval"].status, TaskStatus::Success);
    assert_eq!(run.info.ctx["tenant_token_copy"], REDACTED);
    assert!(!run.durable_text.contains(TOKEN));
    assert!(!run.durable_text.contains("tenant-7f3a9c21"));
}
