use super::*;

#[test]
fn live_final_result_moves_large_values_without_copying_them_into_history() {
    let text = "x".repeat(3 * 1024 * 1024);
    let pointer = text.as_ptr();
    let context = Arc::new(Context::from([
        ("large".into(), Value::String(text)),
        ("small".into(), Value::from(7)),
    ]));
    let (snapshot, live) = prepare_final_context(context, &ExecutionOverlay::default(), true);
    let live = live.unwrap();
    assert_eq!(live["large"].as_str().unwrap().as_ptr(), pointer);
    assert_eq!(snapshot["large"]["_truncated"], true);
    assert_eq!(snapshot["small"], live["small"]);
}

#[test]
fn public_final_snapshot_does_not_retain_live_values() {
    let context = Arc::new(Context::from([(
        "large".into(),
        Value::String("x".repeat(3 * 1024 * 1024)),
    )]));
    let (snapshot, live) = prepare_final_context(context, &ExecutionOverlay::default(), false);
    assert!(live.is_none());
    assert_eq!(snapshot["large"]["_truncated"], true);
}

#[test]
fn bound_context_truncates_only_oversized_values() {
    // IF-048: the persisted final context caps individual oversized values
    // (default limit 2 MiB) while preserving small ones.
    let mut context = Context::new();
    context.insert("small".to_string(), serde_json::json!("hello"));
    context.insert(
        "big".to_string(),
        serde_json::Value::String("x".repeat(3 * 1024 * 1024)),
    );

    let bounded = bound_context(context);
    assert_eq!(bounded.get("small").unwrap(), &serde_json::json!("hello"));
    assert_eq!(
        bounded.get("big").unwrap().get("_truncated").unwrap(),
        &serde_json::json!(true)
    );
}

#[test]
fn oversized_task_output_keeps_the_original_context_allocation() {
    let text = "x".repeat(3 * 1024 * 1024);
    let pointer = text.as_ptr();
    let prepared = prepare_output(
        Context::from([("big".to_string(), Value::String(text))]),
        &ExecutionOverlay::default(),
    );

    assert_eq!(prepared.context["big"].as_str().unwrap().as_ptr(), pointer);
    assert_eq!(prepared.task_value["_truncated"], true);
}

#[test]
fn small_task_output_is_preserved_in_context_and_history() {
    let output = Context::from([
        ("count".to_string(), serde_json::json!(3)),
        ("message".to_string(), serde_json::json!("complete")),
    ]);

    let prepared = prepare_output(output.clone(), &ExecutionOverlay::default());

    assert_eq!(prepared.context, output);
    assert_eq!(
        prepared.task_value,
        serde_json::json!({"count": 3, "message": "complete"})
    );
}

#[test]
fn uniquely_owned_final_context_reuses_value_allocation() {
    let text = "x".repeat(4096);
    let pointer = text.as_ptr();
    let context = Arc::new(Context::from([("result".to_string(), Value::String(text))]));

    let context = into_owned_context(context);

    assert_eq!(context["result"].as_str().unwrap().as_ptr(), pointer);
}

#[test]
fn structured_failure_output_is_taken_without_cloning() {
    let text = "x".repeat(4096);
    let pointer = text.as_ptr();
    let mut failure = anyhow::Error::new(NodeFailure::new(
        "failed",
        Context::from([("diagnostic".to_string(), Value::String(text))]),
    ));

    let prepared = prepare_failure_output(&mut failure, &ExecutionOverlay::default()).unwrap();

    assert_eq!(
        prepared.context["diagnostic"].as_str().unwrap().as_ptr(),
        pointer
    );
    assert!(
        failure
            .downcast_ref::<NodeFailure>()
            .unwrap()
            .output()
            .is_empty()
    );
}

#[test]
fn structured_failure_output_is_redacted_before_publication() {
    let secret = "if022-secret-sentinel";
    let overlay = ExecutionOverlay::new(Context::from([(
        "_headers".to_string(),
        serde_json::json!({"authorization": secret}),
    )]));
    let mut failure = anyhow::Error::new(NodeFailure::new(
        "command failed",
        Context::from([
            ("shell_stderr".to_string(), serde_json::json!(secret)),
            ("shell_stdout".to_string(), serde_json::json!("safe")),
            ("_headers".to_string(), serde_json::json!({"copy": secret})),
        ]),
    ));

    let prepared = prepare_failure_output(&mut failure, &overlay).unwrap();
    let serialized = serde_json::to_string(&prepared.context).unwrap();

    assert!(!serialized.contains(secret));
    assert!(!prepared.context.contains_key("_headers"));
    assert_eq!(prepared.context["shell_stderr"], "[REDACTED]");
    assert_eq!(prepared.context["shell_stdout"], "safe");
}

fn numeric_header_overlay() -> ExecutionOverlay {
    ExecutionOverlay::new(Context::from([(
        "_headers".to_string(),
        serde_json::json!({
            "x-request-timestamp": "1731400000",
            "x-feature-flags": "mode=needs_review"
        }),
    )]))
}

#[test]
fn published_output_keeps_types_and_routes_while_history_is_scrubbed() {
    // IF-176: the live half feeds later steps and the scheduler.
    let prepared = prepare_output(
        Context::from([
            ("_route_gate".to_string(), serde_json::json!("needs_review")),
            ("event_time".to_string(), serde_json::json!(1731400000)),
            ("copy".to_string(), serde_json::json!("at 1731400000")),
            ("count".to_string(), serde_json::json!(3)),
        ]),
        &numeric_header_overlay(),
    );

    assert_eq!(prepared.context["_route_gate"], "needs_review");
    assert_eq!(prepared.context["event_time"], 1731400000);
    assert_eq!(prepared.context["copy"], "at [REDACTED]");
    assert_eq!(
        prepared.task_value,
        serde_json::json!({
            "_route_gate": "[REDACTED]",
            "event_time": "[REDACTED]",
            "copy": "at [REDACTED]",
            "count": 3
        })
    );
}

#[test]
fn child_result_keeps_types_while_its_stored_snapshot_is_scrubbed() {
    let context = Arc::new(Context::from([
        ("_route_gate".to_string(), serde_json::json!("needs_review")),
        ("event_time".to_string(), serde_json::json!(1731400000)),
        ("copy".to_string(), serde_json::json!("1731400000")),
        ("_headers".to_string(), serde_json::json!({"copy": "x"})),
    ]));

    let (snapshot, live) = prepare_final_context(context.clone(), &numeric_header_overlay(), true);
    let live = live.unwrap();

    assert_eq!(live["_route_gate"], "needs_review");
    assert_eq!(live["event_time"], 1731400000);
    assert_eq!(live["copy"], "[REDACTED]");
    assert!(!live.contains_key("_headers"));
    let stored = serde_json::to_string(&snapshot).unwrap();
    assert!(!stored.contains("1731400000"), "{stored}");
    assert!(!stored.contains("needs_review"), "{stored}");
    assert_eq!(snapshot["event_time"], "[REDACTED]");

    // A top-level run has no live reader: everything gets the record form.
    let (durable, live) = prepare_final_context(context, &numeric_header_overlay(), false);
    assert!(live.is_none());
    assert_eq!(durable, snapshot);
}
