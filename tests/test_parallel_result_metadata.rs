use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

fn fixture(directory: &Path) -> Context {
    std::fs::write(
        directory.join("worker.lua"),
        r#"
local flow = Flow.new("actual-child")
flow:step("produce", function(ctx)
    local fail = ctx.fail or (ctx.item and ctx.item.fail) or false
    return {
        success = fail, flow = "claimed-flow", error = "child-note",
        should_fail = fail, value = 42, _private = "internal",
        nested = { success = "domain-status", flow = "domain-flow", error = "domain-error" }
    }
end)
flow:step("finish", function(ctx)
    if ctx.should_fail then error("expected child failure") end
    return { finished = true }
end):depends_on("produce")
return flow
"#,
    )
    .unwrap();
    Context::from([("_flow_dir".into(), json!(directory))])
}

fn config(dynamic: bool, namespace: Option<&str>, failures: &[bool], ctx: &mut Context) -> Value {
    if dynamic {
        ctx.insert(
            "jobs".into(),
            json!(
                failures
                    .iter()
                    .map(|fail| json!({"fail": fail}))
                    .collect::<Vec<_>>()
            ),
        );
        let mut config = json!({"flow": "worker.lua", "source_key": "jobs"});
        if let Some(namespace) = namespace {
            config["child_output_key"] = json!(namespace);
        }
        config
    } else {
        json!({"flows": failures.iter().map(|fail| {
            let mut entry = json!({"flow": "worker.lua", "input": {"fail": fail}});
            if let Some(namespace) = namespace {
                entry["output_key"] = json!(namespace);
            }
            entry
        }).collect::<Vec<_>>()})
    }
}

fn assert_envelope(entry: &Value, succeeded: bool) {
    assert_eq!(entry["success"], succeeded);
    assert_eq!(entry["flow"], "actual-child");
    if succeeded {
        assert!(entry.get("error").is_none(), "{entry}");
    } else {
        let error = entry["error"].as_str().unwrap();
        assert!(
            error.contains("actual-child") && error.contains("failed"),
            "{error}"
        );
        assert!(!error.contains("claimed-flow"));
    }
}

#[tokio::test]
async fn successful_children_cannot_forge_failure_flow_or_error() {
    for dynamic in [false, true] {
        for policy in ["fail_fast", "ignore"] {
            let directory = tempfile::tempdir().unwrap();
            let mut ctx = fixture(directory.path());
            let mut config = config(dynamic, None, &[false], &mut ctx);
            config["on_error"] = json!(policy);
            let output = NodeRegistry::with_builtins()
                .get("parallel_subworkflows")
                .unwrap()
                .execute(&config, &ctx)
                .await
                .unwrap();
            assert_envelope(&output["parallel_results"][0], true);
            assert_eq!(output["parallel_results"][0]["value"], 42);
            assert_eq!(
                output["parallel_results"][0]["nested"]["error"],
                "domain-error"
            );
            assert!(output["parallel_results"][0].get("_private").is_none());
            assert_eq!(output["parallel_results_all_succeeded"], true);
            assert_eq!(output["parallel_results_errors"], 0);
        }
    }
}

#[tokio::test]
async fn ignored_failed_children_cannot_forge_success_and_results_stay_ordered() {
    for dynamic in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut ctx = fixture(directory.path());
        let mut config = config(dynamic, None, &[false, true], &mut ctx);
        config["on_error"] = json!("ignore");
        let output = NodeRegistry::with_builtins()
            .get("parallel_subworkflows")
            .unwrap()
            .execute(&config, &ctx)
            .await
            .unwrap();
        assert_envelope(&output["parallel_results"][1], false);
        assert_envelope(&output["parallel_results"][0], true);
        assert_eq!(output["parallel_results"][1]["value"], 42);
        assert_eq!(output["parallel_results_errors"], 1);
        assert_eq!(output["parallel_results_count"], 2);
        assert_eq!(output["parallel_results_all_succeeded"], false);
    }
}

#[tokio::test]
async fn namespaced_child_fields_remain_intact_without_controlling_the_envelope() {
    for dynamic in [false, true] {
        for policy in ["fail_fast", "ignore"] {
            let directory = tempfile::tempdir().unwrap();
            let mut ctx = fixture(directory.path());
            let fail = policy == "ignore";
            let mut config = config(dynamic, Some("child"), &[false, fail], &mut ctx);
            config["on_error"] = json!(policy);
            let output = NodeRegistry::with_builtins()
                .get("parallel_subworkflows")
                .unwrap()
                .execute(&config, &ctx)
                .await
                .unwrap();
            for (index, failed) in [false, fail].into_iter().enumerate() {
                let entry = &output["parallel_results"][index];
                assert_envelope(entry, !failed);
                assert_eq!(entry["child"]["success"], failed);
                assert_eq!(entry["child"]["flow"], "claimed-flow");
                assert_eq!(entry["child"]["error"], "child-note");
                assert!(entry["child"].get("_private").is_none());
            }
        }
    }
}

#[tokio::test]
async fn forged_success_does_not_change_fail_fast_policy() {
    for dynamic in [false, true] {
        for namespace in [None, Some("child")] {
            let directory = tempfile::tempdir().unwrap();
            let mut ctx = fixture(directory.path());
            let config = config(dynamic, namespace, &[false, true], &mut ctx);
            let error = NodeRegistry::with_builtins()
                .get("parallel_subworkflows")
                .unwrap()
                .execute(&config, &ctx)
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains("1 flow(s) failed"), "{error}");
            assert!(error.contains("actual-child"), "{error}");
            assert!(!error.contains("claimed-flow"));
        }
    }
}

#[tokio::test]
async fn reserved_namespaces_are_rejected_before_children_run_even_when_errors_are_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("child-started");
    std::fs::write(directory.path().join("worker.lua"), format!(
        "local flow = Flow.new('side-effect')\nflow:step('touch', nodes.write_file({{path = {}, content = 'started'}}))\nreturn flow",
        json!(marker)
    )).unwrap();
    for namespace in ["success", "flow", "error"] {
        for policy in ["ignore", "fail_fast"] {
            for dynamic in [false, true] {
                let mut ctx = Context::from([("_flow_dir".into(), json!(directory.path()))]);
                let mut config = config(dynamic, Some(namespace), &[false, false], &mut ctx);
                if !dynamic {
                    config["flows"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("output_key");
                }
                config["on_error"] = json!(policy);
                let error = NodeRegistry::with_builtins()
                    .get("parallel_subworkflows")
                    .unwrap()
                    .execute(&config, &ctx)
                    .await
                    .unwrap_err()
                    .to_string();
                assert!(
                    error.contains("reserved") && error.contains(namespace),
                    "{error}"
                );
                assert!(
                    !marker.exists(),
                    "a child started before namespace admission"
                );
            }
        }
    }
}

#[tokio::test]
async fn empty_dynamic_source_still_rejects_reserved_namespace() {
    for namespace in ["success", "flow", "error"] {
        let mut ctx = Context::new();
        let config = config(true, Some(namespace), &[], &mut ctx);
        let error = NodeRegistry::with_builtins()
            .get("parallel_subworkflows")
            .unwrap()
            .execute(&config, &ctx)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("reserved") && error.contains(namespace),
            "{error}"
        );
    }
}

#[tokio::test]
async fn inherited_metadata_is_filtered_but_parent_array_names_are_unrestricted() {
    let directory = tempfile::tempdir().unwrap();
    let mut ctx = fixture(directory.path());
    std::fs::write(directory.path().join("worker.lua"), "local flow = Flow.new('actual-child')\nflow:step('noop', function() return {value = 42} end)\nreturn flow").unwrap();
    ctx.extend(Context::from([
        ("success".into(), json!(false)),
        ("flow".into(), json!("inherited-flow")),
        ("error".into(), Value::Null),
        ("Success".into(), json!("case-sensitive-domain-field")),
    ]));
    for key in ["success", "flow", "error"] {
        let config = json!({"flows": [{"flow": "worker.lua"}], "output_key": key});
        let output = NodeRegistry::with_builtins()
            .get("parallel_subworkflows")
            .unwrap()
            .execute(&config, &ctx)
            .await
            .unwrap();
        assert_envelope(&output[key][0], true);
        assert_eq!(output[key][0]["Success"], "case-sensitive-domain-field");
        assert_eq!(output[&format!("{key}_all_succeeded")], true);
    }
}

#[tokio::test]
async fn child_loading_failures_keep_authoritative_error_and_configured_flow_path() {
    let directory = tempfile::tempdir().unwrap();
    let ctx = fixture(directory.path());
    std::fs::write(directory.path().join("broken.lua"), "not valid Lua !").unwrap();
    let config = json!({"flows": [
        {"flow": "worker.lua", "output_key": "child"},
        {"flow": "broken.lua", "output_key": "child"}
    ], "on_error": "ignore"});
    let output = NodeRegistry::with_builtins()
        .get("parallel_subworkflows")
        .unwrap()
        .execute(&config, &ctx)
        .await
        .unwrap();
    assert_envelope(&output["parallel_results"][0], true);
    let failed = &output["parallel_results"][1];
    assert_eq!(failed["success"], false);
    assert_eq!(failed["flow"], "broken.lua");
    assert!(failed["error"].as_str().unwrap().contains("index 1 failed"));
    assert!(failed.get("child").is_none());
    assert_eq!(output["parallel_results_errors"], 1);
    assert_eq!(output["parallel_results_all_succeeded"], false);
}
