use std::path::Path;
use std::sync::Arc;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{Context, RunStatus};
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::{StateStore, json_store::JsonStateStore};
use serde_json::{Value, json};

const BYTES: usize = 3 * 1024 * 1024;
const PRODUCE: &str = r#"
flow:step("produce", function()
    return { payload = string.rep("x", 3 * 1024 * 1024), _private = "not-exported" }
end)
flow:step("measure", function(ctx) return { length = #ctx.payload } end):depends_on("produce")
"#;

fn write_flow(directory: &Path, name: &str, body: &str) -> String {
    let path = directory.join(format!("{name}.lua"));
    std::fs::write(
        &path,
        format!("local flow = Flow.new({name:?})\n{body}\nreturn flow"),
    )
    .unwrap();
    path.to_string_lossy().into_owned()
}

fn context(directory: &Path) -> Context {
    Context::from([("_flow_dir".into(), json!(directory))])
}

fn assert_payload(value: &Value) {
    let text = value
        .as_str()
        .expect("live result must remain a string, not a truncation marker");
    assert_eq!(text.len(), BYTES);
    assert!(text.bytes().all(|byte| byte == b'x'));
}

#[tokio::test]
async fn child_matches_inline_values_while_inspection_remains_truncated() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_flow(directory.path(), "payload", PRODUCE);
    let registry = Arc::new(NodeRegistry::with_builtins());
    let flow = LuaRuntime::load_flow_async(&path, &registry).await.unwrap();
    let store = Arc::new(JsonStateStore::new(directory.path().join("state")));
    let engine = WorkflowEngine::new(registry.clone(), store.clone(), None);
    let id = engine
        .execute(&flow, context(directory.path()))
        .await
        .unwrap();
    let info = store.get_run_info(&id).await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx["length"], BYTES);
    assert_eq!(info.ctx["payload"]["_truncated"], true);
    assert_eq!(
        info.tasks["produce"].output.as_ref().unwrap()["_truncated"],
        true
    );

    for namespaced in [false, true] {
        let mut config = json!({"flow": "payload.lua", "on_error": "fail_fast"});
        if namespaced {
            config["output_key"] = json!("child");
        }
        let output = registry
            .get("subworkflow")
            .unwrap()
            .execute(&config, &context(directory.path()))
            .await
            .unwrap();
        let result = if namespaced {
            output["child"].clone()
        } else {
            serde_json::to_value(output).unwrap()
        };
        assert_payload(&result["payload"]);
        assert_eq!(result["length"], info.ctx["length"]);
        assert!(result.get("_private").is_none());
    }
}

#[tokio::test]
async fn parallel_children_return_full_results_in_order() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(directory.path(), "payload", PRODUCE);
    let config = json!({"flows": [{"flow": "payload.lua"}, {"flow": "payload.lua", "output_key": "child"}], "max_concurrent": 2});
    let output = NodeRegistry::with_builtins()
        .get("parallel_subworkflows")
        .unwrap()
        .execute(&config, &context(directory.path()))
        .await
        .unwrap();
    assert_eq!(output["parallel_results_all_succeeded"], true);
    assert_payload(&output["parallel_results"][0]["payload"]);
    assert_payload(&output["parallel_results"][1]["child"]["payload"]);
    assert!(output["parallel_results"][0].get("_private").is_none());
    assert!(
        output["parallel_results"][1]["child"]
            .get("_private")
            .is_none()
    );
}

#[tokio::test]
async fn dynamic_fanout_preserves_full_results_and_source_order() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(directory.path(), "payload", PRODUCE);
    let mut ctx = context(directory.path());
    ctx.insert("jobs".into(), json!([{"id": "first"}, {"id": "second"}]));
    let config = json!({
        "flow": "payload.lua", "source_key": "jobs", "input": {},
        "child_output_key": "child", "max_concurrent": 2
    });
    let output = NodeRegistry::with_builtins()
        .get("parallel_subworkflows")
        .unwrap()
        .execute(&config, &ctx)
        .await
        .unwrap();
    assert_eq!(output["parallel_results_all_succeeded"], true);
    for (index, id) in ["first", "second"].into_iter().enumerate() {
        let child = &output["parallel_results"][index]["child"];
        assert_payload(&child["payload"]);
        assert_eq!(child["item"]["id"], id);
        assert_eq!(child["index"], index + 1);
        assert!(child.get("_private").is_none());
    }
}

#[tokio::test]
async fn repeat_carries_untruncated_state_to_the_next_iteration() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        directory.path(),
        "repeat",
        r#"
flow:step("advance", function(ctx)
    if ctx.repeat_iteration == 1 then
        return { repeat_done = false, repeat_next_state = string.rep("x", 3 * 1024 * 1024) }
    end
    assert(type(ctx.repeat_state) == "string", "carried state was truncated")
    assert(#ctx.repeat_state == 3 * 1024 * 1024, "carried bytes changed")
    return { repeat_done = true, repeat_next_state = ctx.repeat_state }
end)
"#,
    );
    let output = NodeRegistry::with_builtins()
        .get("repeat_subworkflow")
        .unwrap()
        .execute(
            &json!({"flow": "repeat.lua", "max_iterations": 2}),
            &context(directory.path()),
        )
        .await
        .unwrap();
    assert_eq!(output["repeat_result_completed"], true);
    assert_eq!(output["repeat_result_iterations"], 2);
    assert_payload(&output["repeat_result_state"]);
    assert_payload(&output["repeat_result"]["repeat_next_state"]);
}

#[tokio::test]
async fn tool_dispatch_uses_full_child_values_for_results_and_messages() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        directory.path(),
        "tool",
        r#"flow:step("respond", function()
        return { tool_result_value = string.rep("x", 3 * 1024 * 1024) }
    end)"#,
    );
    let mut ctx = context(directory.path());
    ctx.insert(
        "calls".into(),
        json!([{"id": "call1", "name": "large", "arguments": {}}]),
    );
    let output = NodeRegistry::with_builtins().get("tool_dispatch").unwrap().execute(
        &json!({"source_key": "calls", "output_key": "tools", "tools": {"large": {"flow": "tool.lua"}}}), &ctx,
    ).await.unwrap();
    assert_eq!(output["tools_all_succeeded"], true);
    assert_payload(&output["tools"][0]["result"]);
    assert_payload(&output["tools_by_id"]["call1"]["result"]);
    assert_payload(&output["tools_messages"][0]["content"]);
}

#[tokio::test]
async fn ignored_child_failure_keeps_full_prior_values_and_failure_flags() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        directory.path(),
        "partial",
        &format!(
            "{PRODUCE}\nflow:step('fail', function() error('expected failure') end):depends_on('measure')"
        ),
    );
    let registry = NodeRegistry::with_builtins();
    let ctx = context(directory.path());
    let config = json!({"flow": "partial.lua", "output_key": "child", "on_error": "ignore"});
    let output = registry
        .get("subworkflow")
        .unwrap()
        .execute(&config, &ctx)
        .await
        .unwrap();
    assert_eq!(output["child_success"], false);
    assert_eq!(output["subworkflow_success"], false);
    assert!(output["child_error"].as_str().unwrap().contains("failed"));
    assert_payload(&output["child"]["payload"]);
    let config =
        json!({"flows": [{"flow": "partial.lua", "output_key": "child"}], "on_error": "ignore"});
    let output = registry
        .get("parallel_subworkflows")
        .unwrap()
        .execute(&config, &ctx)
        .await
        .unwrap();
    assert_eq!(output["parallel_results_all_succeeded"], false);
    assert_eq!(output["parallel_results"][0]["success"], false);
    assert_payload(&output["parallel_results"][0]["child"]["payload"]);
}
