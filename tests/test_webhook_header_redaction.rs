//! IF-176: forwarded webhook headers must not rewrite live step output.
//!
//! A forwarded header stays execution-only and its value stays out of durable
//! records (IF-011), but short components of the value, header names and
//! engine routing keys are not secrets, and live JSON types are never changed.

use std::collections::HashMap;

use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use ironflow::engine::types::{RunInfo, TaskStatus};
use ironflow::storage::StateStore as _;
use ironflow::storage::event_store::EventStore as _;

use crate::webhook_support::{
    PROCESS_ADMISSION_LOCK, TestApp, authenticated_json_request, authenticated_request,
    build_test_app, send_json, setup_flow_dir, webhook, write_flow,
};

const REDACTED: &str = "[REDACTED]";

/// Everything a run left behind: the stored record, the raw state files, the
/// event log and the public run detail.
struct RunEvidence {
    status: String,
    info: RunInfo,
    durable_text: String,
}

async fn run_webhook(
    flow_source: &str,
    header: (&'static str, &'static str),
    body: &str,
) -> RunEvidence {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = setup_flow_dir();
    write_flow(flows.path(), "flow.lua", flow_source);
    let app: TestApp = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("hook".to_string(), webhook("flow.lua", &[header.0]))]),
    );

    let mut request = authenticated_json_request("/webhooks/hook", body);
    request.headers_mut().insert(
        HeaderName::from_static(header.0),
        HeaderValue::from_static(header.1),
    );
    let (status, response) = send_json(&app.router, request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let run_id = response["run_id"].as_str().unwrap().to_string();

    let info = app.store.get_run_info(&run_id).await.unwrap();
    let (detail_status, detail) = send_json(
        &app.router,
        authenticated_request(Method::GET, &format!("/runs/{run_id}"), Body::empty()),
    )
    .await;
    assert_eq!(detail_status, StatusCode::OK);
    let events = app.events.list_since(&run_id, None, 1000).await.unwrap();

    let mut durable_text = format!(
        "{response}\n{detail}\n{}\n",
        serde_json::to_string(&events).unwrap()
    );
    append_files(app.store_dir.path(), &mut durable_text);
    assert!(durable_text.contains(&run_id), "state files were not read");

    RunEvidence {
        status: response["status"].as_str().unwrap().to_string(),
        info,
        durable_text,
    }
}

fn append_files(directory: &std::path::Path, text: &mut String) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            append_files(&path, text);
        } else {
            text.push_str(&String::from_utf8_lossy(&std::fs::read(&path).unwrap()));
            text.push('\n');
        }
    }
}

#[tokio::test]
async fn feature_flag_header_does_not_suppress_a_guarded_step() {
    const FLAGS: &str = "beta=true,legacy=false";
    let run = run_webhook(
        r#"
        local flow = Flow.new("if176_feature_flags")
        flow:step("observe", function(ctx)
            return { flags_copy = ctx._headers["x-feature-flags"], beta = "true" }
        end)
        flow:step_if("ctx.amount > 100", "needs_approval", function(ctx)
            return { approval_requested = true }
        end):depends_on("observe")
        return flow
        "#,
        ("x-feature-flags", FLAGS),
        r#"{"amount": 250}"#,
    )
    .await;

    assert_eq!(run.status, "success");
    let guard = run.info.tasks["_if_needs_approval"]
        .output
        .as_ref()
        .unwrap();
    assert_eq!(guard["_route__if_needs_approval"], "true");
    assert_eq!(guard["_condition_result__if_needs_approval"], true);
    assert_eq!(run.info.tasks["needs_approval"].status, TaskStatus::Success);
    assert_eq!(run.info.ctx["approval_requested"], true);
    assert_eq!(run.info.ctx["amount"], 250);
    // A short component of the header is not a secret.
    assert_eq!(run.info.ctx["beta"], "true");
    // The whole forwarded value still is (IF-011).
    assert_eq!(run.info.ctx["flags_copy"], REDACTED);
    assert!(!run.durable_text.contains(FLAGS));
    assert!(!run.durable_text.contains("\"_headers\""));
}

#[tokio::test]
async fn trace_header_does_not_corrupt_numbers_keys_or_unrelated_text() {
    const TRACE: &str =
        "Root=1-67891233-abcdef012345678912345678;Parent=53995c3f42cd8ad8;Sampled=1";
    let run = run_webhook(
        r#"
        local flow = Flow.new("if176_trace_header")
        flow:step("build", function(ctx)
            local trace = ctx._headers["x-amzn-trace-id"]
            return {
                order1 = "first",
                total = 1,
                items = { 1, 2, 3 },
                note = "room 101, 1 item",
                sampled = "1",
                trace_copy = trace,
                parent_copy = string.match(trace, "Parent=(%x+)")
            }
        end)
        flow:step("consume", function(ctx)
            return {
                summary = ctx.order1 .. "|" .. ctx.note,
                doubled_total = ctx.total * 2,
                item_sum = ctx.items[1] + ctx.items[2] + ctx.items[3]
            }
        end):depends_on("build")
        return flow
        "#,
        ("x-amzn-trace-id", TRACE),
        "{}",
    )
    .await;

    assert_eq!(run.status, "success");
    let built = run.info.tasks["build"].output.as_ref().unwrap();
    assert_eq!(built["order1"], "first");
    assert_eq!(built["total"], 1);
    assert_eq!(built["items"], serde_json::json!([1, 2, 3]));
    assert_eq!(built["note"], "room 101, 1 item");
    assert_eq!(built["sampled"], "1");
    // The next step read the live values, not rewritten ones.
    assert_eq!(run.info.ctx["summary"], "first|room 101, 1 item");
    assert_eq!(run.info.ctx["doubled_total"], 2);
    assert_eq!(run.info.ctx["item_sum"], 6);
    // The whole value and its long components are still withheld (IF-011).
    assert_eq!(built["trace_copy"], REDACTED);
    assert_eq!(built["parent_copy"], REDACTED);
    assert!(!run.durable_text.contains(TRACE));
    assert!(!run.durable_text.contains("53995c3f42cd8ad8"));
    assert!(!run.durable_text.contains("\"_headers\""));
}

#[tokio::test]
async fn sender_cannot_suppress_a_route_by_sending_its_label() {
    const FLAGS: &str = "mode=needs_review";
    let run = run_webhook(
        r#"
        local flow = Flow.new("if176_route_label")
        flow:step("gate", nodes.if_node({
            condition = "ctx.amount > 100",
            true_route = "needs_review",
            false_route = "auto_accept"
        }))
        flow:step("review", function(ctx)
            return { reviewed = true }
        end):depends_on("gate"):route("needs_review")
        flow:step("accept", function(ctx)
            return { accepted = true }
        end):depends_on("gate"):route("auto_accept")
        return flow
        "#,
        ("x-feature-flags", FLAGS),
        r#"{"amount": 250}"#,
    )
    .await;

    assert_eq!(run.status, "success");
    assert_eq!(run.info.tasks["review"].status, TaskStatus::Success);
    assert_eq!(run.info.tasks["accept"].status, TaskStatus::Skipped);
    assert_eq!(run.info.ctx["reviewed"], true);
    assert!(!run.durable_text.contains(FLAGS));
}

#[tokio::test]
async fn numeric_header_keeps_live_numbers_and_stays_out_of_records() {
    const TIMESTAMP: &str = "1731400000";
    let run = run_webhook(
        r#"
        local flow = Flow.new("if176_numeric_header")
        flow:step("extract", function(ctx)
            return { event_time = ctx.event_time, attempts = 17314000 }
        end)
        flow:step("consume", function(ctx)
            return {
                event_time_is_number = type(ctx.event_time) == "number",
                next_second = ctx.event_time + 1
            }
        end):depends_on("extract")
        return flow
        "#,
        ("x-request-timestamp", TIMESTAMP),
        r#"{"event_time": 1731400000}"#,
    )
    .await;

    assert_eq!(run.status, "success");
    // Live output kept its JSON type between steps.
    assert_eq!(run.info.ctx["event_time_is_number"], true);
    assert_eq!(run.info.ctx["next_second"], 1_731_400_001_i64);
    // A different number is not a match.
    assert_eq!(run.info.ctx["attempts"], 17_314_000);
    // Durable records withhold the number that equals the header value.
    assert_eq!(run.info.ctx["event_time"], REDACTED);
    assert_eq!(
        run.info.tasks["extract"].output.as_ref().unwrap()["event_time"],
        REDACTED
    );
    assert!(!run.durable_text.contains(TIMESTAMP));
}
