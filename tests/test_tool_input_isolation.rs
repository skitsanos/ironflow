use std::fs;
use std::time::Duration;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

const PRIVATE: &str = "private-sibling-sentinel";

fn call(normalized: bool) -> Value {
    if normalized {
        json!({"id": "call-1", "index": 0, "type": "function", "name": "inspect",
               "arguments": {"city": "Berlin"}, "raw_arguments": "{\"city\":\"Berlin\"}"})
    } else {
        json!({"id": "call-1", "type": "function",
               "function": {"name": "inspect", "arguments": "{\"city\":\"Berlin\"}"}})
    }
}

#[tokio::test]
async fn missing_nested_tool_inputs_do_not_disclose_private_siblings_in_results() {
    let directory = tempfile::tempdir().unwrap();
    let node = NodeRegistry::with_builtins().get("tool_dispatch").unwrap();
    for explicit_result in [true, false] {
        let body = if explicit_result {
            "return {tool_result_value = ctx.payload}"
        } else {
            "return {handled = true}"
        };
        fs::write(directory.path().join("inspect.lua"), format!(
            "local flow = Flow.new('inspect')\nflow:step('echo', function(ctx) {body} end)\nreturn flow"
        )).unwrap();
        for normalized in [true, false] {
            for policy in ["fail_fast", "ignore"] {
                let context = Context::from([
                    ("_flow_dir".into(), json!(directory.path())),
                    ("calls".into(), json!([call(normalized)])),
                    (
                        "user".into(),
                        json!({"profile": {"name": "Ada"}, "private_token": PRIVATE}),
                    ),
                ]);
                let original = context.clone();
                let config = json!({
                    "source_key": "calls", "on_error": policy,
                    "tools": {"inspect": {"flow": "inspect.lua", "input": {"payload": {
                        "missing_id": "ctx.user.id", "name": "ctx.user.profile.name",
                        "missing_deep": "ctx.user.profile.id", "missing_root": "ctx.absent.id",
                        "wrong_type": "ctx.user.profile.name.id", "city": "arguments.city",
                        "literal": "unchanged literal", "values": [false, 0, null],
                        "nested": [{"missing": "ctx.user.id"}]
                    }}}}
                });
                let output =
                    tokio::time::timeout(Duration::from_secs(15), node.execute(&config, &context))
                        .await
                        .expect("tool dispatch timed out")
                        .unwrap();
                let serialized = serde_json::to_string(&output).unwrap();
                assert!(
                    !serialized.contains(PRIVATE),
                    "private sibling reached tool output: {serialized}"
                );
                assert_eq!(context, original);
                assert_eq!(output["tool_results_count"], 1);
                assert_eq!(output["tool_results_all_succeeded"], true);
                assert_eq!(output["tool_results_errors"], 0);
                let result = &output["tool_results"][0]["result"];
                let payload = if explicit_result {
                    result
                } else {
                    &result["payload"]
                };
                assert_eq!(
                    *payload,
                    json!({
                        "missing_id": null, "name": "Ada", "missing_deep": null,
                        "missing_root": null, "wrong_type": null, "city": "Berlin",
                        "literal": "unchanged literal", "values": [false, 0, null],
                        "nested": [{"missing": null}]
                    })
                );
                let message = &output["tool_results_messages"][0];
                assert_eq!(message["role"], "tool");
                assert_eq!(message["tool_call_id"], "call-1");
                assert_eq!(
                    serde_json::from_str::<Value>(message["content"].as_str().unwrap()).unwrap(),
                    *result
                );
                assert_eq!(
                    output["tool_results_by_id"]["call-1"],
                    output["tool_results"][0]
                );
            }
        }
    }
}

#[tokio::test]
async fn handlers_can_reject_required_missing_values_without_disclosing_parent_data() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("required.lua"),
        r#"
        local flow = Flow.new("required")
        flow:step("check", function(ctx)
            assert(ctx.user_id ~= json_null, "user_id is required")
            return {tool_result_value = ctx.user_id}
        end)
        return flow
    "#,
    )
    .unwrap();
    let context = Context::from([
        ("_flow_dir".into(), json!(directory.path())),
        ("calls".into(), json!([call(true)])),
        ("user".into(), json!({"private_token": PRIVATE})),
    ]);
    let node = NodeRegistry::with_builtins().get("tool_dispatch").unwrap();
    for policy in ["fail_fast", "ignore"] {
        let config = json!({"source_key": "calls", "on_error": policy,
            "tools": {"inspect": {"flow": "required.lua", "input": {"user_id": "ctx.user.id"}}}});
        let result = tokio::time::timeout(Duration::from_secs(15), node.execute(&config, &context))
            .await
            .expect("tool dispatch timed out");
        if policy == "fail_fast" {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("finished with status: failed"), "{error}");
            assert!(!error.contains(PRIVATE));
        } else {
            let output = result.unwrap();
            assert_eq!(output["tool_results_all_succeeded"], false);
            assert_eq!(output["tool_results_errors"], 1);
            assert!(
                output["tool_results"][0]["result"]
                    .get("user_id")
                    .unwrap()
                    .is_null()
            );
            assert!(!serde_json::to_string(&output).unwrap().contains(PRIVATE));
        }
    }
}

#[tokio::test]
async fn offline_projection_example_validates_and_runs_through_cli() {
    let directory = tempfile::tempdir().unwrap();
    for (action, name) in [
        ("validate", "tool_dispatch_input_projection"),
        ("validate", "tool_input_projection_subworkflow"),
        ("run", "tool_dispatch_input_projection"),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("examples/13-ai/{name}.lua"));
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
        command
            .env_clear()
            .current_dir(directory.path())
            .arg(action)
            .arg(path)
            .kill_on_drop(true);
        if action == "run" {
            command
                .arg("--store-dir")
                .arg(directory.path().join("store"));
        }
        let output = tokio::time::timeout(Duration::from_secs(30), command.output())
            .await
            .expect("tool example CLI timed out")
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{action} {name}: {stdout}\n{stderr}"
        );
        if action == "run" {
            let (_, encoded) = stdout
                .split_once("\nContext:\n")
                .expect("CLI context missing");
            let context: Value = serde_json::from_str(encoded.trim()).unwrap();
            assert_eq!(context["projection_verified"], true);
            assert_eq!(context["user"]["private_note"], "synthetic-private-field");
            assert_eq!(
                context["tool_results"][0]["result"],
                json!({
                    "user_id": null, "display_name": "Ada", "city": "Berlin"
                })
            );
            for key in [
                "tool_results",
                "tool_results_messages",
                "tool_results_by_id",
            ] {
                assert!(
                    !context[key].to_string().contains("synthetic-private-field"),
                    "{key}"
                );
            }
        }
    }
}
