use std::fs;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn write_flow(path: &std::path::Path, name: &str, body: &str) {
    let source = format!(
        r#"
        local flow = Flow.new("{}")
        {}
        return flow
    "#,
        name, body
    );
    fs::write(path, source).unwrap();
}

#[test]
fn tool_dispatch_registered_with_correct_type() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("tool_dispatch");
    assert!(node.is_some(), "tool_dispatch should be registered");
    assert_eq!(node.unwrap().node_type(), "tool_dispatch");
}

#[tokio::test]
async fn tool_dispatch_runs_mapped_subworkflow_for_normalized_call() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("tool_dispatch").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let weather_path = dir.path().join("weather.lua");
    write_flow(
        &weather_path,
        "weather_tool",
        r#"
        flow:step("respond", function(ctx)
            return {
                tool_result_text = "Weather for " .. ctx.city,
                tool_result_value = {
                    city = ctx.city,
                    call_id = ctx.tool_call_id,
                    source = "subworkflow"
                }
            }
        end)
        "#,
    );

    let config = serde_json::json!({
        "source_key": "calls",
        "output_key": "tool_results",
        "tools": {
            "get_weather": {
                "flow": "weather.lua",
                "input": {
                    "city": "arguments.city"
                }
            }
        }
    });
    let mut ctx = ctx_with(vec![(
        "calls",
        serde_json::json!([{
            "id": "call_1",
            "index": 0,
            "type": "function",
            "name": "get_weather",
            "arguments": { "city": "Berlin" },
            "raw_arguments": "{\"city\":\"Berlin\"}"
        }]),
    )]);
    ctx.insert(
        "_flow_dir".to_string(),
        serde_json::Value::String(dir.path().to_string_lossy().to_string()),
    );

    let out = node.execute(&config, &ctx).await.unwrap();
    assert_eq!(out.get("tool_results_count").unwrap(), 1);
    assert_eq!(out.get("tool_results_errors").unwrap(), 0);
    assert_eq!(out.get("tool_results_all_succeeded").unwrap(), true);

    let results = out.get("tool_results").and_then(|v| v.as_array()).unwrap();
    assert_eq!(results[0].get("success").unwrap(), true);
    assert_eq!(results[0].get("id").unwrap(), "call_1");
    assert_eq!(results[0].get("name").unwrap(), "get_weather");
    assert_eq!(
        results[0].get("result").unwrap().get("city").unwrap(),
        "Berlin"
    );
    assert_eq!(
        results[0].get("result").unwrap().get("call_id").unwrap(),
        "call_1"
    );

    let messages = out
        .get("tool_results_messages")
        .and_then(|v| v.as_array())
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].get("role").unwrap(), "tool");
    assert_eq!(messages[0].get("tool_call_id").unwrap(), "call_1");

    let by_id = out.get("tool_results_by_id").unwrap();
    assert_eq!(
        by_id
            .get("call_1")
            .unwrap()
            .get("result")
            .unwrap()
            .get("source")
            .unwrap(),
        "subworkflow"
    );
}

#[tokio::test]
async fn tool_dispatch_accepts_raw_tool_calls_and_reports_unknown_when_ignored() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("tool_dispatch").unwrap();

    let config = serde_json::json!({
        "source_key": "calls",
        "output_key": "tool_results",
        "on_error": "ignore",
        "tools": {}
    });
    let ctx = ctx_with(vec![(
        "calls",
        serde_json::json!([{
            "id": "call_missing",
            "type": "function",
            "function": {
                "name": "missing_tool",
                "arguments": "{\"x\":1}"
            }
        }]),
    )]);

    let out = node.execute(&config, &ctx).await.unwrap();
    assert_eq!(out.get("tool_results_count").unwrap(), 1);
    assert_eq!(out.get("tool_results_errors").unwrap(), 1);
    assert_eq!(out.get("tool_results_all_succeeded").unwrap(), false);

    let results = out.get("tool_results").and_then(|v| v.as_array()).unwrap();
    assert_eq!(results[0].get("success").unwrap(), false);
    assert_eq!(results[0].get("name").unwrap(), "missing_tool");
    assert!(
        results[0]
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap()
            .contains("unsupported tool")
    );
}
