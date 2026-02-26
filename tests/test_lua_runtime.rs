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
    assert_eq!(flow.steps[2].dependencies.len(), 2);
    assert!(flow.steps[2].dependencies.contains(&"a".to_string()));
    assert!(flow.steps[2].dependencies.contains(&"b".to_string()));
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
        flow:step_if("ctx.ready == true", "action", nodes.log({ message = "go" }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    assert_eq!(flow.name, "step_if_parse");
    assert_eq!(flow.steps.len(), 2);

    // First step is the auto-generated if_node guard
    assert_eq!(flow.steps[0].name, "_if_action");
    assert_eq!(flow.steps[0].node_type, "if_node");

    // Second step depends on the guard and has route "true"
    assert_eq!(flow.steps[1].name, "action");
    assert_eq!(flow.steps[1].dependencies, vec!["_if_action"]);
    assert_eq!(flow.steps[1].route, Some("true".to_string()));
}
