use ironflow::engine::types::Context;
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

fn large_context() -> Context {
    Context::from([
        ("unused".into(), json!(vec![1; 200_000])),
        ("count".into(), json!(200_000)),
    ])
}

fn config(source: &str) -> Value {
    let registry = NodeRegistry::with_builtins();
    let flow = LuaRuntime::load_flow_from_string(source, &registry).unwrap();
    flow.steps.last().unwrap().config.clone()
}

#[tokio::test]
async fn source_projection_excludes_large_unused_context() {
    let registry = NodeRegistry::with_builtins();
    let ctx = large_context();
    let node = registry.get("code").unwrap();
    let result = node
        .execute(
            &json!({
                "context_keys": ["count"],
                "source": "assert(ctx.unused == nil); return { count = ctx.count }"
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(result["count"], 200_000);
    assert_eq!(ctx["unused"].as_array().unwrap().len(), 200_000);

    let result = node
        .execute(
            &json!({"context_keys": [], "source": "assert(next(ctx) == nil); return 7"}),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(result["result"], 7);
}

#[tokio::test]
async fn selected_large_values_and_omitted_projection_still_fail() {
    let registry = NodeRegistry::with_builtins();
    let ctx = large_context();
    for config in [
        json!({"source": "return 7"}),
        json!({"context_keys": ["unused"], "source": "return ctx.unused[1]"}),
        json!({"context_keys": ["unused"], "source": "return 7"}),
    ] {
        let error = registry
            .get("code")
            .unwrap()
            .execute(&config, &ctx)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("JSON-to-Lua maximum node count"), "{error}");
        assert!(error.contains("$.unused["), "{error}");
        assert!(error.contains("context_keys"), "{error}");
    }
}

#[tokio::test]
async fn function_projection_still_rejects_selected_large_values() {
    let registry = NodeRegistry::with_builtins();
    let config = config(
        r#"
        local flow = Flow.new("selected")
        flow:step("inspect", function(ctx)
            return ctx.unused[1]
        end):context_keys({"unused"})
        return flow
    "#,
    );
    let error = registry
        .get("code")
        .unwrap()
        .execute(&config, &large_context())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("JSON-to-Lua maximum node count"), "{error}");
    assert!(error.contains("$.unused["), "{error}");
}

#[tokio::test]
async fn inline_function_and_step_builders_share_projection() {
    let registry = NodeRegistry::with_builtins();
    let sources = [
        r#"
        local flow = Flow.new("inline")
        flow:step("inspect", nodes.code({
            context_keys = {"count"},
            source = function(argument)
                assert(argument == ctx and ctx.unused == nil)
                return { count = argument.count }
            end
        }))
        return flow
        "#,
        r#"
        local flow = Flow.new("handler")
        flow:step("inspect", function(argument)
            assert(argument == ctx and ctx.unused == nil)
            return { count = argument.count }
        end):context_keys({"count"}):retries(1):timeout(2)
        return flow
        "#,
        r#"
        local flow = Flow.new("conditional")
        flow:step("prepare", nodes.code({source = "return 1"}))
        flow:step_if("true", "inspect", function(argument)
            assert(argument == ctx and ctx.unused == nil)
            return { count = argument.count }
        end):context_keys({"count"}):depends_on("prepare")
        return flow
        "#,
    ];
    for source in sources {
        let config = config(source);
        assert_eq!(config["context_keys"], json!(["count"]));
        let result = registry
            .get("code")
            .unwrap()
            .execute(&config, &large_context())
            .await
            .unwrap();
        assert_eq!(result["count"], 200_000);
    }
}

#[tokio::test]
async fn empty_lua_projection_is_not_an_omitted_projection() {
    let registry = NodeRegistry::with_builtins();
    for step in [
        "flow:step('empty', function(ctx) assert(next(ctx) == nil); return 7 end):context_keys({})",
        "flow:step('empty', nodes.code({context_keys = {}, source = 'assert(next(ctx) == nil); return 7'}))",
    ] {
        let source = format!("local flow = Flow.new('empty')\n{step}\nreturn flow");
        let config = config(&source);
        assert_eq!(config["context_keys"], json!([]));
        let result = registry
            .get("code")
            .unwrap()
            .execute(&config, &large_context())
            .await
            .unwrap();
        assert_eq!(result["result"], 7);
    }
}

#[tokio::test]
async fn projection_preserves_literal_keys_nulls_and_isolated_snapshots() {
    let registry = NodeRegistry::with_builtins();
    let ctx = Context::from([
        (
            "a.b".into(),
            json!({"value": 1, "items": [], "missing": null}),
        ),
        ("a".into(), json!({"b": "not selected"})),
        ("${ctx.name}".into(), json!("literal")),
    ]);
    let result = registry
        .get("code")
        .unwrap()
        .execute(
            &json!({
                "context_keys": ["a.b", "missing", "a.b", "${ctx.name}"],
                "source": r#"
                assert(ctx.a == nil and ctx.missing == nil)
                assert(ctx["${ctx.name}"] == "literal")
                ctx["a.b"].value = 2
                return { selected = ctx["a.b"] }
            "#
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        result["selected"],
        json!({"value": 2, "items": [], "missing": null})
    );
    assert_eq!(ctx["a.b"]["value"], 1);
}

#[tokio::test]
async fn projection_rejects_invalid_json_selectors() {
    let registry = NodeRegistry::with_builtins();
    for keys in [
        json!(null),
        json!("count"),
        json!({}),
        json!([1]),
        json!(["count", false]),
    ] {
        let error = registry
            .get("code")
            .unwrap()
            .execute(
                &json!({"context_keys": keys, "source": "return 1"}),
                &large_context(),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("context_keys"), "{error}");
    }
}

#[test]
fn projection_rejects_invalid_lua_selectors_during_load_and_validation() {
    let registry = NodeRegistry::with_builtins();
    for keys in [
        "'count'",
        "false",
        "{true}",
        "{name = 'count'}",
        "{[2] = 'count'}",
        "json_null",
    ] {
        for step in [
            format!("flow:step('bad', function() return 1 end):context_keys({keys})"),
            format!("flow:step('bad', nodes.code({{context_keys = {keys}, source = 'return 1'}}))"),
        ] {
            let source = format!("local flow = Flow.new('bad')\n{step}\nreturn flow");
            let error = LuaRuntime::load_flow_from_string(&source, &registry)
                .unwrap_err()
                .to_string();
            assert!(error.contains("context_keys"), "{error}");
            assert!(LuaRuntime::validate_flow_from_string(&source, &registry).is_err());
        }
    }
}

#[test]
fn projection_rejects_unsupported_builders_and_late_config_mutations() {
    let registry = NodeRegistry::with_builtins();
    for step in [
        "flow:step('log', nodes.log({message = 'ok'})):context_keys({})",
        "local node = nodes.code({source = 'return 1'}); flow:step('bad', node); node.context_keys = false",
    ] {
        let source = format!("local flow = Flow.new('bad')\n{step}\nreturn flow");
        let error = LuaRuntime::load_flow_from_string(&source, &registry)
            .unwrap_err()
            .to_string();
        assert!(error.contains("context_keys"), "{error}");
    }
}

#[test]
fn builder_projection_does_not_mutate_reused_descriptors_or_guards() {
    let registry = NodeRegistry::with_builtins();
    let flow = LuaRuntime::load_flow_from_string(
        r#"
        local flow = Flow.new("reuse")
        local node = nodes.code({source = "return 1", context_keys = {"original"}})
        flow:step("first", node):context_keys({"first"})
        flow:step("second", node)
        flow:step_if("true", "third", node):context_keys({}):depends_on("first")
        return flow
        "#,
        &registry,
    )
    .unwrap();
    assert_eq!(flow.steps[0].config["context_keys"], json!(["first"]));
    assert_eq!(flow.steps[1].config["context_keys"], json!(["original"]));
    assert!(flow.steps[2].config.get("context_keys").is_none());
    assert_eq!(flow.steps[2].dependencies, ["first"]);
    assert_eq!(flow.steps[3].config["context_keys"], json!([]));
    assert_eq!(flow.steps[3].dependencies, ["_if_third"]);
}

#[tokio::test]
async fn default_foreach_setup_retains_cumulative_context_limits() {
    let registry = NodeRegistry::with_builtins();
    let config = config(
        r#"
        local flow = Flow.new("foreach_default")
        flow:step("map", nodes.foreach({
            source_key = "items",
            transform = function(item) return item end
        }))
        return flow
    "#,
    );
    let mut ctx = Context::from([
        ("items".into(), json!([1])),
        ("a".into(), json!(vec![0; 60_000])),
    ]);
    assert!(
        registry
            .get("foreach")
            .unwrap()
            .execute(&config, &ctx)
            .await
            .is_ok()
    );
    ctx.insert("b".into(), json!(vec![0; 60_000]));
    let error = registry
        .get("foreach")
        .unwrap()
        .execute(&config, &ctx)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("JSON-to-Lua maximum node count"), "{error}");
}

#[test]
fn descriptor_projection_on_non_code_nodes_fails_load_and_validation() {
    let registry = NodeRegistry::with_builtins();
    for node in [
        r#"nodes.log({ context_keys = {"x"}, message = "hi" })"#,
        r#"nodes.log({ context_keys = "not-a-list", message = "hi" })"#,
        r#"nodes.log({ context_keys = json_null, message = "hi" })"#,
        r#"nodes.foreach({
            context_keys = {"x"},
            source_key = "items",
            transform = function(item) return item end
        })"#,
    ] {
        let source = format!("local flow = Flow.new('bad')\nflow:step('bad', {node})\nreturn flow");
        let error = LuaRuntime::load_flow_from_string(&source, &registry)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(
                "Step 'bad': context_keys is only supported for code nodes and function handlers"
            ),
            "{error}"
        );
        assert!(
            LuaRuntime::validate_flow_from_string(&source, &registry).is_err(),
            "{node}"
        );
    }
}

/// The CLI injects `_flow_dir`, and a failing step's recovery handler receives
/// the `_error_*` overlay. Neither may be hidden by a user-key projection.
#[test]
fn cli_recovery_handler_projection_keeps_engine_reserved_keys() {
    let temp = tempfile::tempdir().unwrap();
    let flow_path = temp.path().join("flow.lua");
    std::fs::write(
        &flow_path,
        r#"
        local flow = Flow.new("projected_recovery")
        flow:step("risky", function() error("boom") end):on_error("recover")
        flow:step("recover", function(ctx)
            assert(ctx.order_id == nil, "unlisted user keys must stay hidden")
            assert(ctx._error_step == "risky", "missing _error_step")
            assert(ctx._error_node_type == "code", "missing _error_node_type")
            assert(string.find(ctx._error_message, "boom", 1, true), "missing _error_message")
            return { recovered_from = ctx._error_step }
        end):context_keys({"unrelated"})
        flow:step("constant", function(ctx)
            assert(ctx.order_id == nil and ctx.recovered_from == nil)
            assert(type(ctx._flow_dir) == "string", "_flow_dir must survive an empty projection")
            return { ready = true }
        end):context_keys({}):depends_on("recover")
        return flow
        "#,
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .arg("run")
        .arg(&flow_path)
        .arg("--store-dir")
        .arg(temp.path().join("runs"))
        .arg("--context")
        .arg(r#"{"order_id": 42}"#)
        .output()
        .expect("run ironflow CLI");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("Status: success"), "{stdout}");
    let (_, encoded) = stdout
        .split_once("\nContext:\n")
        .expect("CLI context missing");
    let context: Value = serde_json::from_str(encoded.trim()).unwrap();
    assert_eq!(context["recovered_from"], "risky");
    assert_eq!(context["ready"], true);
    assert_eq!(context["order_id"], 42);
    assert!(context.get("_error_message").is_none());
}
