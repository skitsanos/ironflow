//! IF-221: a child input mapping fails the step when its source is absent,
//! before any child run starts, instead of dropping the entry or passing a
//! string literal.

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

/// Every node that takes a parent-key mapping, with `SPEC` standing for the
/// mapping of the child key `value`.
fn parent_key_configs(spec: &Value) -> Vec<(&'static str, Value)> {
    vec![
        (
            "subworkflow",
            json!({"flow": "child.lua", "output_key": "out", "input": {"value": spec}}),
        ),
        (
            "subworkflow",
            json!({"flow": "child.lua", "wait": false, "input": {"value": spec}}),
        ),
        (
            "parallel_subworkflows",
            json!({"flows": [{"flow": "child.lua", "output_key": "out", "input": {"value": spec}}],
                   "on_error": "ignore"}),
        ),
        (
            "parallel_subworkflows",
            json!({"flow": "child.lua", "source_key": "jobs", "child_output_key": "out",
                   "input": {"value": spec}, "on_error": "ignore"}),
        ),
        (
            "repeat_subworkflow",
            json!({"flow": "child.lua", "max_iterations": 1, "input": {"value": spec}}),
        ),
    ]
}

#[tokio::test]
async fn a_missing_parent_key_fails_the_step_before_any_child_runs() {
    for (node, config) in parent_key_configs(&json!("secret-looking-absent-key")) {
        let directory = tempfile::tempdir().unwrap();
        write_child(directory.path());
        let error = execute(node, &config, &parent(directory.path()))
            .await
            .expect_err(node)
            .to_string();
        assert!(
            error.contains(&format!("{node}: input 'value'")) && error.contains("does not exist"),
            "{node}: {error}"
        );
        assert!(
            !error.contains("secret-looking-absent-key"),
            "{node} disclosed the mapping value: {error}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(!child_ran(directory.path()), "{node} started a child");
    }
}

#[tokio::test]
async fn a_present_parent_key_is_mapped_as_before() {
    for (node, config) in parent_key_configs(&json!("present")) {
        if config.get("wait") == Some(&json!(false)) {
            continue;
        }
        let directory = tempfile::tempdir().unwrap();
        write_child(directory.path());
        let output = execute(node, &config, &parent(directory.path()))
            .await
            .unwrap();
        let seen = match node {
            "subworkflow" => &output["out"]["seen"],
            "parallel_subworkflows" => &output["parallel_results"][0]["out"]["seen"],
            _ => &output["repeat_result"]["seen"],
        };
        assert_eq!(*seen, json!({"id": 7}), "{node}");
    }
}

#[tokio::test]
async fn the_optional_form_omits_an_absent_key_and_maps_a_present_one() {
    for (reference, expected, has_value) in [
        ("absent", Value::Null, false),
        ("present", json!({"id": 7}), true),
    ] {
        for (node, config) in parent_key_configs(&json!({"optional": reference})) {
            if config.get("wait") == Some(&json!(false)) {
                continue;
            }
            let directory = tempfile::tempdir().unwrap();
            write_child(directory.path());
            let output = execute(node, &config, &parent(directory.path()))
                .await
                .unwrap();
            let child = match node {
                "subworkflow" => &output["out"],
                "parallel_subworkflows" => &output["parallel_results"][0]["out"],
                _ => &output["repeat_result"],
            };
            assert_eq!(child["has_value"], has_value, "{node} {reference}");
            assert_eq!(
                child.get("seen").cloned().unwrap_or(Value::Null),
                expected,
                "{node} {reference}"
            );
        }
    }
}

#[tokio::test]
async fn the_literal_form_passes_a_value_that_names_a_parent_key_unresolved() {
    for literal in [
        json!("present"),
        json!("no such key"),
        json!({"literal": 1}),
    ] {
        for (node, config) in parent_key_configs(&json!({"literal": literal})) {
            if config.get("wait") == Some(&json!(false)) {
                continue;
            }
            let directory = tempfile::tempdir().unwrap();
            write_child(directory.path());
            let output = execute(node, &config, &parent(directory.path()))
                .await
                .unwrap();
            let seen = match node {
                "subworkflow" => &output["out"]["seen"],
                "parallel_subworkflows" => &output["parallel_results"][0]["out"]["seen"],
                _ => &output["repeat_result"]["seen"],
            };
            assert_eq!(*seen, literal, "{node}");
        }
    }
}

#[tokio::test]
async fn malformed_mappings_are_rejected_before_any_child_runs() {
    let cases = [
        json!({"optional": 7}),
        json!({"optional": ""}),
        json!({"optional": {"key": "present"}}),
    ];
    for spec in cases {
        for (node, config) in parent_key_configs(&spec) {
            let directory = tempfile::tempdir().unwrap();
            write_child(directory.path());
            let error = execute(node, &config, &parent(directory.path()))
                .await
                .expect_err(node)
                .to_string();
            assert!(
                error.contains(&format!("{node}: input 'value'")),
                "{node}: {error}"
            );
            assert!(!child_ran(directory.path()), "{node} started a child");
        }
    }
    // A mapping that is not an object used to mean "inherit the whole parent".
    for (node, mut config) in parent_key_configs(&Value::Null) {
        if config.get("flows").is_some() {
            config["flows"][0]["input"] = json!("present");
        } else {
            config["input"] = json!("present");
        }
        let directory = tempfile::tempdir().unwrap();
        write_child(directory.path());
        let error = execute(node, &config, &parent(directory.path()))
            .await
            .expect_err(node)
            .to_string();
        assert!(
            error.contains("'input' must be an object"),
            "{node}: {error}"
        );
        assert!(!child_ran(directory.path()), "{node} started a child");
    }
}

#[tokio::test]
async fn non_string_values_and_nested_data_stay_literal() {
    let spec = json!({"key": "present", "list": ["present"], "optional": "present", "n": 1});
    for (node, config) in parent_key_configs(&spec) {
        if config.get("wait") == Some(&json!(false)) {
            continue;
        }
        let directory = tempfile::tempdir().unwrap();
        write_child(directory.path());
        let output = execute(node, &config, &parent(directory.path()))
            .await
            .unwrap();
        let seen = match node {
            "subworkflow" => &output["out"]["seen"],
            "parallel_subworkflows" => &output["parallel_results"][0]["out"]["seen"],
            _ => &output["repeat_result"]["seen"],
        };
        assert_eq!(*seen, spec, "{node}");
    }
}
