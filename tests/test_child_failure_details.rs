//! IF-135: live failure details across composition boundaries, without providers.

use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

struct BadRequestServer {
    url: String,
    task: tokio::task::JoinHandle<()>,
}

impl BadRequestServer {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            "/",
            axum::routing::get(|| async { axum::http::StatusCode::BAD_REQUEST }),
        );
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { url, task }
    }
}

impl Drop for BadRequestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn write_flow(directory: &Path, name: &str, body: &str) {
    std::fs::write(
        directory.join(format!("{name}.lua")),
        format!("local flow = Flow.new({name:?})\n{body}\nreturn flow"),
    )
    .unwrap();
}

fn context(directory: &Path) -> Context {
    Context::from([("_flow_dir".to_string(), json!(directory))])
}

fn write_failures(directory: &Path, url: &str) {
    write_flow(
        directory,
        "lua_failure",
        r#"flow:step("explode", function() error("boom") end)"#,
    );
    write_flow(
        directory,
        "http_failure",
        &format!(
            "flow:step('request', nodes.http_get({{url = {url:?}, timeout = 2, proxy_mode = 'direct'}}))"
        ),
    );
}

#[tokio::test]
async fn single_ignore_and_fail_fast_expose_lua_and_http_errors_with_or_without_namespace() {
    let directory = tempfile::tempdir().unwrap();
    let server = BadRequestServer::start().await;
    write_failures(directory.path(), &server.url);
    let node = NodeRegistry::with_builtins().get("subworkflow").unwrap();

    for (name, task, reason) in [
        ("lua_failure", "explode", "boom"),
        ("http_failure", "request", "returned status 400"),
    ] {
        for namespaced in [false, true] {
            let mut config = json!({"flow": format!("{name}.lua"), "on_error": "ignore"});
            if namespaced {
                config["output_key"] = json!("child");
            }
            let output = node
                .execute(&config, &context(directory.path()))
                .await
                .unwrap();
            assert_eq!(output["subworkflow_success"], false);
            let error = output["subworkflow_error"].as_str().unwrap();
            assert!(error.contains(&format!("task '{task}'")), "{error}");
            assert!(error.contains(reason), "{error}");
            if namespaced {
                assert_eq!(output["child_success"], false);
                assert_eq!(output["child_error"], output["subworkflow_error"]);
            }
            config["on_error"] = json!("fail_fast");
            let strict = node
                .execute(&config, &context(directory.path()))
                .await
                .unwrap_err();
            assert_eq!(strict.to_string(), error);
        }
    }
}

#[tokio::test]
async fn parallel_entries_and_fail_fast_include_each_child_reason_in_input_order() {
    let directory = tempfile::tempdir().unwrap();
    let server = BadRequestServer::start().await;
    write_failures(directory.path(), &server.url);
    let node = NodeRegistry::with_builtins()
        .get("parallel_subworkflows")
        .unwrap();

    for namespaced in [false, true] {
        let mut flows = json!([{"flow": "lua_failure.lua"}, {"flow": "http_failure.lua"}]);
        if namespaced {
            for entry in flows.as_array_mut().unwrap() {
                entry["output_key"] = json!("child");
            }
        }
        let mut config = json!({"flows": flows, "on_error": "ignore", "max_concurrent": 2});
        let output = node
            .execute(&config, &context(directory.path()))
            .await
            .unwrap();
        assert_eq!(output["parallel_results_errors"], 2);
        assert_eq!(output["parallel_results_all_succeeded"], false);
        let results = output["parallel_results"].as_array().unwrap();
        let mut errors = Vec::new();
        for (index, reason) in ["boom", "returned status 400"].into_iter().enumerate() {
            assert_eq!(results[index]["success"], false);
            let error = results[index]["error"].as_str().unwrap();
            assert!(error.contains(reason), "{error}");
            assert!(error.contains(&format!("index {index}")), "{error}");
            errors.push(error);
        }
        config["on_error"] = json!("fail_fast");
        let strict = node
            .execute(&config, &context(directory.path()))
            .await
            .unwrap_err();
        assert_eq!(
            strict.to_string(),
            format!(
                "parallel_subworkflows: 2 flow(s) failed:\n{}",
                errors.join("\n")
            )
        );
    }
}

#[tokio::test]
async fn multiple_unresolved_errors_are_sorted_and_recovered_children_have_no_error() {
    let directory = tempfile::tempdir().unwrap();
    let recovered = r#"
        flow:step("source", function() error("recovered-reason") end):on_error("recover")
        flow:step("recover", function() return { repaired = true } end)
    "#;
    write_flow(directory.path(), "recovered", recovered);
    write_flow(
        directory.path(),
        "mixed",
        &format!(
            r#"{recovered}
            flow:step("zeta", function() error("zeta-reason") end)
            flow:step("alpha", function() error("alpha-reason") end)
        "#
        ),
    );
    let registry = NodeRegistry::with_builtins();
    let single = registry.get("subworkflow").unwrap();
    for namespaced in [false, true] {
        let mut config = json!({"flow": "recovered.lua", "on_error": "ignore"});
        if namespaced {
            config["output_key"] = json!("child");
        }
        let output = single
            .execute(&config, &context(directory.path()))
            .await
            .unwrap();
        assert_eq!(output["subworkflow_success"], true);
        // The error slot stays present so a later run overwrites an earlier one.
        assert_eq!(output["subworkflow_error"], Value::Null);
        assert_eq!(
            output.get("child_error"),
            namespaced.then_some(&Value::Null),
            "{output:?}"
        );
    }
    let parallel = registry.get("parallel_subworkflows").unwrap();
    let config = json!({
        "flows": [{"flow": "mixed.lua"}, {"flow": "recovered.lua"}],
        "on_error": "ignore"
    });
    let output = parallel
        .execute(&config, &context(directory.path()))
        .await
        .unwrap();
    assert_eq!(output["parallel_results_errors"], 1);
    let entries = &output["parallel_results"];
    let error = entries[0]["error"].as_str().unwrap();
    assert!(error.contains("alpha-reason"), "{error}");
    assert!(error.contains("zeta-reason"), "{error}");
    assert!(error.find("task 'alpha'").unwrap() < error.find("task 'zeta'").unwrap());
    assert!(!error.contains("recovered-reason"), "{error}");
    assert_eq!(entries[1]["success"], true);
    assert!(entries[1].get("error").is_none());
}

#[tokio::test]
async fn later_success_resets_error_and_nested_metadata_does_not_leak() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        directory.path(),
        "lua_failure",
        r#"flow:step("explode", function() error("boom") end)"#,
    );
    write_flow(
        directory.path(),
        "success",
        r#"flow:step("ok", function() return { ok = true } end)"#,
    );
    // Node outputs merge into the run context by key, so the second call must
    // overwrite the first call's error instead of leaving it beside success.
    write_flow(
        directory.path(),
        "sequential",
        r#"
        flow:step("first", nodes.subworkflow({ flow = "lua_failure.lua", on_error = "ignore" }))
        flow:step("second", nodes.subworkflow({ flow = "success.lua", on_error = "ignore" }))
            :depends_on("first")
        flow:step("check", function(ctx)
            assert(ctx.subworkflow_success == true, "second child success was hidden")
            assert(ctx.subworkflow_name == "success", "stale subworkflow_name")
            assert(ctx.subworkflow_error ~= nil, "subworkflow_error slot is missing")
            assert(ctx.subworkflow_error == json_null, "stale subworkflow_error")
            assert(type(ctx.subworkflow_error) ~= "string", "stale subworkflow_error")
            return { sequential_verified = true }
        end):depends_on("second")
        "#,
    );
    // A child that tolerated a failing grandchild, and detached another one,
    // carries that metadata in its own context; the parent must not inherit it.
    write_flow(
        directory.path(),
        "nested",
        r#"
        flow:step("inner", nodes.subworkflow({ flow = "lua_failure.lua", on_error = "ignore" }))
        flow:step("detached", nodes.subworkflow({ flow = "success.lua", wait = false }))
            :depends_on("inner")
        flow:step("after", function(ctx)
            assert(type(ctx.subworkflow_error) == "string", "grandchild error was lost")
            return { nested_done = true }
        end):depends_on("detached")
        "#,
    );
    let node = NodeRegistry::with_builtins().get("subworkflow").unwrap();

    let sequential = node
        .execute(
            &json!({"flow": "sequential.lua", "output_key": "run", "on_error": "fail_fast"}),
            &context(directory.path()),
        )
        .await
        .unwrap();
    assert_eq!(sequential["run"]["sequential_verified"], true);
    assert_eq!(sequential["run"]["subworkflow_success"], true);
    assert_eq!(sequential["run"]["subworkflow_error"], Value::Null);
    assert_eq!(sequential["run_success"], true);
    assert_eq!(sequential["run_error"], Value::Null);
    assert_eq!(sequential["subworkflow_error"], Value::Null);

    let nested = node
        .execute(&json!({"flow": "nested.lua"}), &context(directory.path()))
        .await
        .unwrap();
    assert_eq!(nested["nested_done"], true);
    assert_eq!(nested["subworkflow_name"], "nested");
    assert_eq!(nested["subworkflow_success"], true);
    assert_eq!(nested["subworkflow_error"], Value::Null);
    assert!(!nested.contains_key("subworkflow_async"), "{nested:?}");
}

#[tokio::test]
async fn repeat_and_tool_dispatch_preserve_child_reasons() {
    let directory = tempfile::tempdir().unwrap();
    write_flow(
        directory.path(),
        "failure",
        r#"flow:step("explode", function() error("boom") end)"#,
    );
    let registry = NodeRegistry::with_builtins();
    let repeat = registry.get("repeat_subworkflow").unwrap();
    let error = repeat
        .execute(
            &json!({"flow": "failure.lua", "max_iterations": 2}),
            &context(directory.path()),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("iteration 1 finished with status: failed"),
        "{error}"
    );
    assert!(error.contains("boom"), "{error}");

    let mut ctx = context(directory.path());
    ctx.insert(
        "calls".to_string(),
        json!([{
            "id": "call-1", "name": "tool", "arguments": {}, "raw_arguments": "{}"
        }]),
    );
    let tool = registry.get("tool_dispatch").unwrap();
    let mut config = json!({
        "source_key": "calls", "tools": {"tool": {"flow": "failure.lua"}}, "on_error": "ignore"
    });
    let output = tool.execute(&config, &ctx).await.unwrap();
    let error = output["tool_results"][0]["error"].as_str().unwrap();
    assert!(error.contains("boom"), "{error}");
    assert_eq!(output["tool_results"][0]["success"], false);
    assert_eq!(
        output["tool_results_by_id"]["call-1"]["error"],
        Value::from(error)
    );
    config["on_error"] = json!("fail_fast");
    assert_eq!(
        tool.execute(&config, &ctx).await.unwrap_err().to_string(),
        error
    );
}

#[tokio::test]
async fn offline_child_failure_example_validates_and_runs() {
    let directory = tempfile::tempdir().unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/11-subworkflow");
    for name in ["if135_child_error_details.lua", "if135_error_child.lua"] {
        for action in ["validate", "run"] {
            let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
            command
                .env_clear()
                .current_dir(directory.path())
                .kill_on_drop(true)
                .env("TMPDIR", directory.path())
                .env("IRONFLOW_ARTIFACT_DIR", directory.path().join("artifacts"))
                .arg(action)
                .arg(examples.join(name));
            if action == "validate" {
                command.arg("--strict");
            }
            let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
                .await
                .expect("child failure example CLI did not settle")
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success(),
                "{name} {action}: {stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            if action == "run" && name == "if135_child_error_details.lua" {
                let ctx: Value =
                    serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim())
                        .unwrap();
                assert_eq!(ctx["child_error_details_verified"], true);
                assert_eq!(ctx["documents_errors"], 1);
            }
        }
    }
}
