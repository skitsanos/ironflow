//! IF-221: a `tool_dispatch` input mapping fails the step when a parent
//! context reference is absent, before any handler runs.

use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

/// A child that leaves a marker file when it runs and echoes its context.
fn write_child(directory: &Path) {
    let marker = json!(directory.join("child-ran"));
    std::fs::write(
        directory.join("child.lua"),
        format!(
            "local flow = Flow.new('child')\n\
             flow:step('mark', nodes.write_file({{ path = {marker}, content = 'ran' }}))\n\
             flow:step('echo', function(ctx)\n\
                 return {{ seen = ctx.value, has_value = ctx.value ~= nil,\n\
                          repeat_done = true, tool_result_value = {{ seen = ctx.value }} }}\n\
             end):depends_on('mark')\n\
             return flow"
        ),
    )
    .unwrap();
}

fn child_ran(directory: &Path) -> bool {
    directory.join("child-ran").exists()
}

fn parent(directory: &Path) -> Context {
    Context::from([
        ("_flow_dir".into(), json!(directory)),
        ("present".into(), json!({"id": 7})),
        ("jobs".into(), json!([1, 2])),
        (
            "calls".into(),
            json!([{"id": "call-1", "index": 0, "type": "function", "name": "inspect",
                    "arguments": {"city": "Berlin"}, "raw_arguments": "{\"city\":\"Berlin\"}"}]),
        ),
    ])
}

async fn execute(node: &str, config: &Value, ctx: &Context) -> anyhow::Result<Context> {
    NodeRegistry::with_builtins()
        .get(node)
        .unwrap()
        .execute(config, ctx)
        .await
}

fn dispatch_config(input: Value, policy: &str) -> Value {
    json!({"source_key": "calls", "on_error": policy,
           "tools": {"inspect": {"flow": "child.lua", "input": input},
                     "unused": {"flow": "child.lua", "input": {"value": "ctx.absent"}}}})
}

#[tokio::test]
async fn tool_dispatch_fails_on_a_missing_parent_reference_before_any_call() {
    for (entry, input) in [
        ("value", json!({"value": "ctx.absent"})),
        ("value", json!({"value": "ctx.present.missing"})),
        ("value", json!({"value": "ctx.present.id.deeper"})),
        ("value", json!({"value": "ctx."})),
        ("value", json!({"value": "secret-looking-bare-name"})),
        (
            "value.nested.0",
            json!({"value": {"nested": ["ctx.absent"]}}),
        ),
    ] {
        for policy in ["fail_fast", "ignore"] {
            let directory = tempfile::tempdir().unwrap();
            write_child(directory.path());
            let mut ctx = parent(directory.path());
            // The failing call is second: the first must not have run.
            let call = ctx["calls"][0].clone();
            let mut other = call.clone();
            other["name"] = json!("first");
            ctx.insert("calls".into(), json!([other, call]));
            let mut config = dispatch_config(input.clone(), policy);
            config["tools"]["first"] = json!({"flow": "child.lua"});
            let error = execute("tool_dispatch", &config, &ctx)
                .await
                .expect_err(entry)
                .to_string();
            assert!(
                error.contains(&format!("tool_dispatch: tool 'inspect': input '{entry}'"))
                    && error.contains("does not exist"),
                "{input}: {error}"
            );
            assert!(!error.contains("secret-looking"), "{error}");
            assert!(!child_ran(directory.path()), "{input} started a handler");
        }
    }
}

#[tokio::test]
async fn tool_dispatch_optional_and_literal_forms() {
    let directory = tempfile::tempdir().unwrap();
    write_child(directory.path());
    let ctx = parent(directory.path());
    let input = json!({"value": {
        "absent": {"optional": "ctx.absent"},
        "absent_field": {"optional": "ctx.present.missing"},
        "absent_bare": {"optional": "nothing_here"},
        "found": {"optional": "ctx.present.id"},
        "bare": "present",
        "text": {"literal": "present"},
        "path_text": {"literal": "ctx.present.id"},
        "data": {"literal": {"optional": "ctx.present"}},
        "no_argument": "arguments.country",
        "city": "arguments.city",
        "list": [{"literal": "arguments.city"}, 1, null]
    }});
    let output = execute("tool_dispatch", &dispatch_config(input, "fail_fast"), &ctx)
        .await
        .unwrap();
    assert_eq!(
        output["tool_results"][0]["result"]["seen"],
        json!({
            "absent": null, "absent_field": null, "absent_bare": null, "found": 7,
            "bare": {"id": 7}, "text": "present", "path_text": "ctx.present.id",
            "data": {"optional": "ctx.present"}, "no_argument": null, "city": "Berlin",
            "list": ["arguments.city", 1, null]
        })
    );
}

#[tokio::test]
async fn tool_dispatch_rejects_a_malformed_mapping_before_any_call() {
    for input in [
        json!("present"),
        json!({"value": {"optional": 1}}),
        json!({"value": {"optional": ""}}),
    ] {
        let directory = tempfile::tempdir().unwrap();
        write_child(directory.path());
        let error = execute(
            "tool_dispatch",
            &dispatch_config(input.clone(), "ignore"),
            &parent(directory.path()),
        )
        .await
        .expect_err("malformed mapping")
        .to_string();
        assert!(error.contains("tool_dispatch: tool 'inspect'"), "{error}");
        assert!(!child_ran(directory.path()), "{input} started a handler");
    }
}
