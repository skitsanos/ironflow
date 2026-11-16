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
fn structured_failure_output_is_redacted_in_its_record_only() {
    let secret = "if022-secret-sentinel";
    let overlay = ExecutionOverlay::new(Context::from([(
        "_headers".to_string(),
        serde_json::json!({"authorization": secret}),
    )]));
    let output = Context::from([
        ("shell_stderr".to_string(), serde_json::json!(secret)),
        ("shell_stdout".to_string(), serde_json::json!("safe")),
        ("_headers".to_string(), serde_json::json!({"copy": secret})),
    ]);
    let mut failure = anyhow::Error::new(NodeFailure::new("command failed", output.clone()));

    let prepared = prepare_failure_output(&mut failure, &overlay).unwrap();

    // The recovery handler and later steps read what the node returned.
    assert_eq!(prepared.context, output);
    // Task history does not (IF-011).
    let record = serde_json::to_string(&prepared.task_value).unwrap();
    assert!(!record.contains(secret), "{record}");
    assert_eq!(
        prepared.task_value,
        serde_json::json!({"shell_stderr": "[REDACTED]", "shell_stdout": "safe"})
    );
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
fn published_output_is_what_the_step_returned_while_history_is_scrubbed() {
    // IF-219: the live half feeds later steps and the scheduler untouched.
    let output = Context::from([
        ("_route_gate".to_string(), serde_json::json!("needs_review")),
        ("event_time".to_string(), serde_json::json!(1731400000)),
        ("copy".to_string(), serde_json::json!("at 1731400000")),
        ("mode=needs_review".to_string(), serde_json::json!("by key")),
        ("count".to_string(), serde_json::json!(3)),
    ]);

    let prepared = prepare_output(output.clone(), &numeric_header_overlay());

    assert_eq!(prepared.context, output);
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
fn child_result_is_what_the_child_produced_while_its_stored_snapshot_is_scrubbed() {
    let produced = Context::from([
        ("_route_gate".to_string(), serde_json::json!("needs_review")),
        ("event_time".to_string(), serde_json::json!(1731400000)),
        ("copy".to_string(), serde_json::json!("1731400000")),
        ("_headers".to_string(), serde_json::json!({"copy": "x"})),
    ]);
    let context = Arc::new(produced.clone());

    let (snapshot, live) = prepare_final_context(context.clone(), &numeric_header_overlay(), true);

    // The parent step reads the child's values as they are.
    assert_eq!(live.unwrap(), produced);
    let stored = serde_json::to_string(&snapshot).unwrap();
    assert!(!stored.contains("1731400000"), "{stored}");
    assert!(!stored.contains("needs_review"), "{stored}");
    assert!(!stored.contains("_headers"), "{stored}");
    assert_eq!(snapshot["event_time"], "[REDACTED]");
    assert_eq!(snapshot["copy"], "[REDACTED]");

    // A top-level run has no live reader: everything gets the record form.
    let (durable, live) = prepare_final_context(context, &numeric_header_overlay(), false);
    assert!(live.is_none());
    assert_eq!(durable, snapshot);
}

fn nested(depth: usize) -> Value {
    (0..depth).fold(Value::Null, |inner, _| Value::Array(vec![inner]))
}

#[test]
fn output_at_the_storage_depth_limit_is_admitted_and_one_level_deeper_is_refused() {
    let limit = crate::util::json_depth::MAX_JSON_VALUE_DEPTH;
    let admitted = Context::from([("parsed".to_string(), nested(limit))]);
    assert!(admit_output(admitted.clone()).unwrap() == admitted);

    let error =
        admit_output(Context::from([("parsed".to_string(), nested(limit + 1))])).unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "step output refused: value 'parsed' is nested deeper than 123 levels of arrays and \
         objects"
    );
    // The typed cause survives added context, which is what stops a retry.
    assert!(is_depth_refusal(&error.context("Task 'parse' failed")));
    assert!(!is_depth_refusal(&anyhow::anyhow!(
        "nested deeper than 123"
    )));
}

#[test]
fn failure_output_deeper_than_the_storage_limit_is_dropped_from_both_halves() {
    let limit = crate::util::json_depth::MAX_JSON_VALUE_DEPTH;
    let failure = |depth| {
        anyhow::Error::new(NodeFailure::new(
            "provider rejected the request",
            Context::from([("detail".to_string(), nested(depth))]),
        ))
    };

    let mut kept = failure(limit);
    let prepared = prepare_failure_output(&mut kept, &ExecutionOverlay::default()).unwrap();
    assert!(prepared.context["detail"] == nested(limit));

    let mut dropped = failure(limit + 1);
    let prepared = prepare_failure_output(&mut dropped, &ExecutionOverlay::default()).unwrap();
    assert!(prepared.context.is_empty());
    assert_eq!(prepared.task_value["_truncated"], true);
    assert_eq!(prepared.task_value["_limit_depth"], 123);
    let note = prepared.task_value["_note"].as_str().unwrap();
    assert!(
        note.contains("value 'detail' is nested deeper than 123 levels"),
        "{note}"
    );
    // The step's own error is untouched.
    assert_eq!(dropped.to_string(), "provider rejected the request");
}
