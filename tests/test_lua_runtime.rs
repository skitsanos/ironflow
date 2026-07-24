//! Tests for Lua runtime: flow loading, parsing, sandbox security.

use std::io::Write;
use std::sync::Arc;

use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;

fn registry() -> Arc<NodeRegistry> {
    Arc::new(NodeRegistry::with_builtins())
}

// --- load_flow_from_string ---

#[test]
fn load_simple_flow() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("test_flow")
        flow:step("greet", nodes.log({ message = "hello" }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.name, "test_flow");
    assert_eq!(flow.steps.len(), 1);
    assert_eq!(flow.steps[0].name, "greet");
    assert_eq!(flow.steps[0].node_type, "log");
}

#[test]
fn load_flow_with_dependencies() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("deps")
        flow:step("a", nodes.log({ message = "first" }))
        flow:step("b", nodes.log({ message = "second" })):depends_on("a")
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.steps.len(), 2);
    assert!(flow.steps[1].dependencies.contains(&"a".to_string()));
}

#[test]
fn load_flow_preserves_source_declaration_order() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("ordered")
        flow:step("z_first", nodes.log({ message = "first" }))
        flow:step("a_second", nodes.log({ message = "second" }))
        flow:step("m_third", nodes.log({ message = "third" }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    let names: Vec<&str> = flow.steps.iter().map(|step| step.name.as_str()).collect();

    assert_eq!(names, vec!["z_first", "a_second", "m_third"]);
}

#[test]
fn load_flow_with_retries() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("retry_test")
        flow:step("api_call", nodes.log({ message = "test" })):retries(3, 2.0)
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.steps[0].retry.max_retries, 3);
    assert!((flow.steps[0].retry.backoff_s - 2.0).abs() < f64::EPSILON);
}

#[test]
fn load_flow_with_timeout() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("timeout_test")
        flow:step("slow", nodes.log({ message = "test" })):timeout(30)
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.steps[0].timeout_s, Some(30.0));
}

#[test]
fn load_flow_with_route() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("route_test")
        flow:step("check", nodes.if_node({ condition = "ctx.x > 1" }))
        flow:step("branch", nodes.log({ message = "hi" })):depends_on("check"):route("true")
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.steps[1].route.as_deref(), Some("true"));
}

#[test]
fn load_flow_with_on_error() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("error_test")
        flow:step("risky", nodes.log({ message = "try" })):on_error("handler")
        flow:step("handler", nodes.log({ message = "caught" }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.steps[0].on_error.as_deref(), Some("handler"));
    assert!(flow.steps[1].on_error.is_none());
}

#[test]
fn load_flow_with_function_handler() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("func_test")
        flow:step("compute", nodes.code({
            source = function(ctx)
                return { result = 42 }
            end
        }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.steps[0].node_type, "code");
    // Function should be serialized to bytecode_b64
    let config = &flow.steps[0].config;
    assert!(config.get("bytecode_b64").is_some());
}

#[test]
fn load_flow_multiple_depends() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("multi_deps")
        flow:step("a", nodes.log({ message = "a" }))
        flow:step("b", nodes.log({ message = "b" }))
        flow:step("c", nodes.log({ message = "c" })):depends_on("a", "b")
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.steps[2].dependencies, vec!["a", "b"]);
}

#[test]
fn load_flow_chained_builder() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("chained")
        flow:step("s", nodes.log({ message = "x" })):depends_on("a"):retries(2, 0.5):timeout(10):route("yes")
        flow:step("a", nodes.log({ message = "y" }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    let s = &flow.steps[0];
    assert_eq!(s.dependencies, vec!["a"]);
    assert_eq!(s.retry.max_retries, 2);
    assert!((s.retry.backoff_s - 0.5).abs() < f64::EPSILON);
    assert_eq!(s.timeout_s, Some(10.0));
    assert_eq!(s.route.as_deref(), Some("yes"));
}

// --- Duplicate step name detection ---

#[test]
fn duplicate_step_name_errors() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("dup")
        flow:step("a", nodes.log({ message = "1" }))
        flow:step("a", nodes.log({ message = "2" }))
        return flow
    "#;

    let result = LuaRuntime::load_flow_from_string(source, &reg);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Duplicate"), "Error: {}", err);
}

// --- Sandbox security ---

#[test]
fn sandbox_blocks_os() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("evil")
        flow:step("hack", nodes.code({ source = "os.execute('echo pwned')" }))
        return flow
    "#;

    // Loading should succeed (the code isn't executed during parsing)
    let flow = LuaRuntime::load_flow_from_string(source, &reg);
    assert!(flow.is_ok());
}

#[test]
fn sandbox_blocks_io() {
    let reg = registry();
    let source = r#"
        local x = io.open("/etc/passwd")
        local flow = Flow.new("evil")
        return flow
    "#;

    let result = LuaRuntime::load_flow_from_string(source, &reg);
    assert!(result.is_err());
}

#[test]
fn sandbox_does_not_expose_package_loader_or_system_libraries() {
    let reg = registry();
    let source = r#"
        if require ~= nil or package ~= nil or os ~= nil or io ~= nil or
           load ~= nil or loadfile ~= nil or dofile ~= nil or
           collectgarbage ~= nil or string.dump ~= nil then
            error("unsafe Lua library is available")
        end
        local flow = Flow.new("sandboxed")
        return flow
    "#;

    let result = LuaRuntime::load_flow_from_string(source, &reg);
    assert!(result.is_ok(), "sandbox was not sealed: {result:?}");
}

#[tokio::test]
async fn code_node_does_not_expose_package_loader_or_system_libraries() {
    let reg = registry();
    let node = reg.get("code").unwrap();

    let output = node
        .execute(
            &serde_json::json!({
                "source": r#"
                    return {
                        require_type = type(require),
                        package_type = type(package),
                        os_type = type(os),
                        io_type = type(io),
                        load_type = type(load),
                        collectgarbage_type = type(collectgarbage),
                        string_dump_type = type(string.dump)
                    }
                "#
            }),
            &Default::default(),
        )
        .await
        .unwrap();

    for key in [
        "require_type",
        "package_type",
        "os_type",
        "io_type",
        "load_type",
        "collectgarbage_type",
        "string_dump_type",
    ] {
        assert_eq!(output.get(key), Some(&serde_json::json!("nil")), "{key}");
    }
}

#[tokio::test]
async fn code_node_rejects_forged_bytecode() {
    // IF-035: a flow author cannot smuggle arbitrary (memory-unsafe) Lua
    // bytecode via a hand-crafted bytecode_b64 config string. Only bytecode this
    // process compiled and signed may load.
    use base64::Engine as _;
    let reg = registry();
    let node = reg.get("code").unwrap();

    let forged =
        base64::engine::general_purpose::STANDARD.encode(b"\x1bLua forged bytecode payload");
    let err = node
        .execute(
            &serde_json::json!({ "bytecode_b64": forged }),
            &Default::default(),
        )
        .await
        .expect_err("forged bytecode must be rejected")
        .to_string();
    assert!(
        err.contains("authenticat"),
        "expected an authentication failure, got: {err}"
    );
}

#[test]
fn sandbox_keeps_computation_libraries() {
    let reg = registry();
    let source = r#"
        local values = {"beta", "alpha"}
        table.insert(values, "gamma")
        table.sort(values)
        if table.concat(values, ",") ~= "alpha,beta,gamma" then error("table") end
        if string.upper("flow") ~= "FLOW" then error("string") end
        if math.min(4, 2) ~= 2 or math.max(4, 2) ~= 4 then error("math") end
        if utf8.len("IronFlow") ~= 8 then error("utf8") end
        local flow = Flow.new("computation_libraries")
        return flow
    "#;

    let result = LuaRuntime::load_flow_from_string(source, &reg);
    assert!(result.is_ok(), "safe Lua library missing: {result:?}");
}

#[test]
fn sandbox_exposes_new_globals() {
    let reg = registry();
    let source = r#"
        local parsed = json_parse('{\"ok\": 1}')
        if type(parsed) ~= "table" or parsed.ok ~= 1 then
            error("json_parse failed")
        end

        local txt = json_stringify({ok = parsed.ok})
        if type(txt) ~= "string" or string.match(txt, "\"ok\"%s*:%s*1") == nil then
            error("json_stringify failed")
        end

        local id = uuid4()
        if type(id) ~= "string" or #id ~= 36 then
            error("uuid4 failed")
        end

        local ts = now_rfc3339()
        if type(ts) ~= "string" or string.match(ts, "^%d%d%d%d%-") == nil then
            error("now_rfc3339 failed")
        end

        local ms = now_unix_ms()
        if type(ms) ~= "number" then
            error("now_unix_ms failed")
        end

        log("info", "sandbox globals are available")

        local flow = Flow.new("globals")
        flow:step("ping", nodes.log({ message = "ok" }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg);
    assert!(flow.is_ok());
}

#[test]
fn flow_loader_preserves_explicit_json_shapes_and_nulls() {
    let reg = registry();
    let source = r#"
        local parsed = json_parse('{"items":[],"missing":null}')
        local flow = Flow.new("json_shapes")
        flow:step("shape", nodes.log({
            message = "ok",
            empty_array = json_array({}),
            empty_object = json_object({}),
            explicit_null = json_null,
            parsed_array = parsed.items,
            parsed_null = parsed.missing
        }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    let config = &flow.steps[0].config;
    assert_eq!(config.get("empty_array"), Some(&serde_json::json!([])));
    assert_eq!(config.get("empty_object"), Some(&serde_json::json!({})));
    assert_eq!(config.get("explicit_null"), Some(&serde_json::Value::Null));
    assert_eq!(config.get("parsed_array"), Some(&serde_json::json!([])));
    assert_eq!(config.get("parsed_null"), Some(&serde_json::Value::Null));
}

#[test]
fn flow_loader_rejects_mixed_config_tables_with_step_path() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("mixed_config")
        flow:step("bad", nodes.log({ message = "ok", [1] = "not-json" }))
        return flow
    "#;

    let error = LuaRuntime::load_flow_from_string(source, &reg).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("mixed Lua table"), "{message}");
    assert!(message.contains("$.steps[\"bad\"].config"), "{message}");
}

// --- load_flow from file ---

#[test]
fn load_flow_from_file() {
    let reg = registry();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_flow.lua");
    let mut f = std::fs::File::create(&path).unwrap();
    write!(
        f,
        r#"
        local flow = Flow.new("file_flow")
        flow:step("s1", nodes.log({{ message = "from file" }}))
        return flow
    "#
    )
    .unwrap();

    let flow = LuaRuntime::load_flow(&path.to_string_lossy(), &reg).unwrap();
    assert_eq!(flow.name, "file_flow");
    assert_eq!(flow.steps.len(), 1);
}

#[test]
fn load_flow_missing_file() {
    let reg = registry();
    let result = LuaRuntime::load_flow("/nonexistent/path.lua", &reg);
    assert!(result.is_err());
}

// --- Invalid Lua ---

#[test]
fn invalid_lua_syntax() {
    let reg = registry();
    let source = "this is not valid lua!!!";
    let result = LuaRuntime::load_flow_from_string(source, &reg);
    assert!(result.is_err());
}

#[test]
fn flow_without_return() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("no_return")
        flow:step("a", nodes.log({ message = "hi" }))
        -- no return
    "#;

    let result = LuaRuntime::load_flow_from_string(source, &reg);
    assert!(result.is_err());
}

// --- step_if parsing ---

#[test]
fn load_flow_with_step_if() {
    let reg = registry();
    let source = r#"
        local flow = Flow.new("step_if_parse")
        flow:step("prepare", nodes.code({ source = "return { ready = true }" }))
        flow:step_if("ctx.ready == true", "action", nodes.log({ message = "go" }))
            :depends_on("prepare")
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.name, "step_if_parse");
    assert_eq!(flow.steps.len(), 3);

    // User dependencies gate the auto-generated if_node guard.
    assert_eq!(flow.steps[1].name, "_if_action");
    assert_eq!(flow.steps[1].node_type, "if_node");
    assert_eq!(flow.steps[1].dependencies, vec!["prepare"]);

    // The visible step depends only on the guard and has route "true".
    assert_eq!(flow.steps[2].name, "action");
    assert_eq!(flow.steps[2].dependencies, vec!["_if_action"]);
    assert_eq!(flow.steps[2].route, Some("true".to_string()));
}
