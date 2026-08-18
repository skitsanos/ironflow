use std::fs;
use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn flow_context(directory: &Path) -> Context {
    Context::from([(
        "_flow_dir".to_string(),
        serde_json::Value::String(directory.to_string_lossy().to_string()),
    )])
}

fn write_flow(path: &Path, name: &str, body: &str) {
    fs::write(
        path,
        format!(
            r#"
            local flow = Flow.new({name:?})
            {body}
            return flow
            "#
        ),
    )
    .unwrap();
}

fn repeat_node() -> std::sync::Arc<dyn ironflow::nodes::Node> {
    NodeRegistry::with_builtins()
        .get("repeat_subworkflow")
        .expect("repeat_subworkflow must be registered")
}

#[test]
fn repeat_subworkflow_is_registered() {
    assert_eq!(repeat_node().node_type(), "repeat_subworkflow");
}

#[tokio::test]
async fn repeat_subworkflow_carries_only_explicit_state_until_completion() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        &directory.path().join("counter.lua"),
        "counter",
        r#"
        flow:step("advance", function(ctx)
            local next_value = (ctx.loop_state or 0) + 1
            return {
                next_value = next_value,
                finished = next_value >= 3,
                observed_iteration = ctx.turn,
                static_label = ctx.label,
                _private_value = "do-not-publish"
            }
        end)
        "#,
    );

    let mut context = flow_context(directory.path());
    context.insert("seed".to_string(), serde_json::json!(1));
    context.insert("label".to_string(), serde_json::json!("kept"));
    context.insert("unmapped".to_string(), serde_json::json!("excluded"));
    let config = serde_json::json!({
        "flow": "counter.lua",
        "input": {
            "loop_state": "seed",
            "label": "label"
        },
        "state_key": "loop_state",
        "next_state_key": "next_value",
        "until_key": "finished",
        "iteration_key": "turn",
        "max_iterations": 4,
        "output_key": "loop"
    });

    let output = repeat_node().execute(&config, &context).await.unwrap();
    assert_eq!(output["loop_iterations"], 2);
    assert_eq!(output["loop_completed"], true);
    assert_eq!(output["loop_state"], 3);
    assert_eq!(output["loop_flow"], "counter");
    assert_eq!(output["loop"]["observed_iteration"], 2);
    assert_eq!(output["loop"]["static_label"], "kept");
    assert!(output["loop"].get("unmapped").is_none());
    assert!(output["loop"].get("_private_value").is_none());
}

#[tokio::test]
async fn repeat_subworkflow_can_complete_first_iteration_without_state() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        &directory.path().join("once.lua"),
        "once",
        r#"flow:step("done", function(ctx)
            return { repeat_done = true, result = ctx.repeat_iteration }
        end)"#,
    );
    let config = serde_json::json!({
        "flow": "once.lua",
        "max_iterations": 1
    });

    let output = repeat_node()
        .execute(&config, &flow_context(directory.path()))
        .await
        .unwrap();
    assert_eq!(output["repeat_result_iterations"], 1);
    assert_eq!(output["repeat_result_state"], serde_json::Value::Null);
    assert_eq!(output["repeat_result"]["result"], 1);
}

#[tokio::test]
async fn repeat_subworkflow_fails_when_iteration_limit_is_exhausted() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        &directory.path().join("never.lua"),
        "never",
        r#"flow:step("continue", function(ctx)
            return {
                repeat_done = false,
                repeat_next_state = (ctx.repeat_state or 0) + 1
            }
        end)"#,
    );
    let config = serde_json::json!({
        "flow": "never.lua",
        "max_iterations": 2
    });

    let error = repeat_node()
        .execute(&config, &flow_context(directory.path()))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("did not set 'repeat_done' to true"),
        "{error}"
    );
    assert!(error.contains("max_iterations (2)"), "{error}");
}

#[tokio::test]
async fn repeat_subworkflow_requires_completion_and_next_state_contracts() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        &directory.path().join("missing_done.lua"),
        "missing_done",
        r#"flow:step("work", function() return { value = 1 } end)"#,
    );
    write_flow(
        &directory.path().join("missing_state.lua"),
        "missing_state",
        r#"flow:step("work", function() return { repeat_done = false } end)"#,
    );

    let missing_done = repeat_node()
        .execute(
            &serde_json::json!({"flow": "missing_done.lua", "max_iterations": 2}),
            &flow_context(directory.path()),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(missing_done.contains("must return boolean 'repeat_done'"));

    let missing_state = repeat_node()
        .execute(
            &serde_json::json!({"flow": "missing_state.lua", "max_iterations": 2}),
            &flow_context(directory.path()),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(missing_state.contains("omitted 'repeat_next_state'"));
}

#[tokio::test]
async fn repeat_subworkflow_rejects_invalid_bounds_keys_and_input() {
    let context = Context::new();
    for (config, expected) in [
        (
            serde_json::json!({"flow": "x.lua"}),
            "requires 'max_iterations'",
        ),
        (
            serde_json::json!({"flow": "x.lua", "max_iterations": 0}),
            "must be greater than 0",
        ),
        (
            serde_json::json!({"flow": "x.lua", "max_iterations": 1025}),
            "exceeds process limit",
        ),
        (
            serde_json::json!({
                "flow": "x.lua",
                "max_iterations": 1,
                "state_key": "same",
                "until_key": "same"
            }),
            "must be distinct",
        ),
        (
            serde_json::json!({
                "flow": "x.lua",
                "max_iterations": 1,
                "input": {"repeat_done": false}
            }),
            "reserved child key",
        ),
        (
            serde_json::json!({
                "flow": "x.lua",
                "max_iterations": 1,
                "delay_seconds": -1
            }),
            "must be non-negative",
        ),
    ] {
        let error = repeat_node()
            .execute(&config, &context)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(expected),
            "expected {expected:?}, got {error:?}"
        );
    }
}

#[tokio::test]
async fn repeat_subworkflow_is_available_inside_nested_child_registry() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        &directory.path().join("counter.lua"),
        "counter",
        r#"flow:step("advance", function(ctx)
            local value = (ctx.repeat_state or 0) + 1
            return { repeat_next_state = value, repeat_done = value == 2 }
        end)"#,
    );
    write_flow(
        &directory.path().join("outer.lua"),
        "outer",
        r#"flow:step("loop", nodes.repeat_subworkflow({
            flow = "counter.lua",
            max_iterations = 2,
            output_key = "nested_loop"
        }))"#,
    );
    let subworkflow = NodeRegistry::with_builtins().get("subworkflow").unwrap();
    let output = subworkflow
        .execute(
            &serde_json::json!({
                "flow": "outer.lua",
                "output_key": "outer_result",
                "on_error": "fail_fast"
            }),
            &flow_context(directory.path()),
        )
        .await
        .unwrap();

    assert_eq!(output["outer_result"]["nested_loop_iterations"], 2);
    assert_eq!(output["outer_result"]["nested_loop_state"], 2);
}
