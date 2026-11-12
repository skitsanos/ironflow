//! IF-175: flow source cannot size an allocation or drive unbounded recursion
//! while a flow is loaded or validated; every such input is an ordinary error.

use std::sync::Arc;

use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;

/// Most steps one flow may declare, and most dependencies one step may list.
const MAX_FLOW_STEPS: usize = 10_000;
/// Stack size of the server threads that load and validate flows.
const WORKER_STACK_BYTES: usize = 2 * 1024 * 1024;

fn registry() -> Arc<NodeRegistry> {
    Arc::new(NodeRegistry::with_builtins())
}

fn load_error(source: &str) -> String {
    let error = LuaRuntime::load_flow_from_string(source, &registry()).unwrap_err();
    format!("{error:#}")
}

/// Run `work` on a thread no larger than a server worker. A stack overflow
/// there aborts the test process, which is the failure this guards against.
fn on_worker_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(WORKER_STACK_BYTES)
        .spawn(work)
        .unwrap()
        .join()
        .unwrap()
}

fn chained_expression(terms: usize) -> String {
    format!("1{}", "+1".repeat(terms))
}

fn flow_with_top_level_expression(terms: usize) -> String {
    format!(
        "local v = {}\nlocal flow = Flow.new('deep')\nflow:step('a', nodes.log({{ message = 'x' }}))\nreturn flow\n",
        chained_expression(terms)
    )
}

#[test]
fn forged_step_count_is_a_load_error() {
    let error =
        load_error(r#"return { _name = "x", _steps = {}, _step_count = 1000000000000000 }"#);
    assert!(error.contains("invalid step count"), "{error}");
}

#[test]
fn step_count_must_match_the_steps_the_flow_holds() {
    for forged in ["0", "2", "'1'", "1.5", "nil"] {
        let source = format!(
            "local flow = Flow.new('mismatch')\nflow:step('a', nodes.log({{ message = 'x' }}))\nflow._step_count = {forged}\nreturn flow"
        );
        let error = load_error(&source);
        assert!(error.contains("invalid step count"), "{forged}: {error}");
    }
}

#[test]
fn length_metamethods_on_engine_owned_sequences_are_a_load_error() {
    let cases = [
        ("flow._steps[1].dependencies", "Step 'a'"),
        ("flow._steps", "steps"),
    ];
    for (target, subject) in cases {
        let source = format!(
            "local flow = Flow.new('forged')\nflow:step('a', nodes.log({{ message = 'x' }}))\nsetmetatable({target}, {{ __len = function() return 1000000000000000 end }})\nreturn flow"
        );
        let error = load_error(&source);
        assert!(
            error.contains(subject) && error.contains("metatable"),
            "{target}: {error}"
        );
    }
}

#[test]
fn depends_on_appends_after_the_real_dependencies() {
    // The builder must not ask a script-defined `__len` where to write.
    let source = r#"
        local flow = Flow.new("append")
        local step = flow:step("a", nodes.log({ message = "x" }))
        step:depends_on("first")
        local dependencies = flow._steps[1].dependencies
        setmetatable(dependencies, { __len = function() return 1000000000000000 end })
        step:depends_on("second")
        setmetatable(dependencies, nil)
        return flow
    "#;
    let flow = LuaRuntime::load_flow_from_string(source, &registry()).unwrap();
    assert_eq!(flow.steps[0].dependencies, ["first", "second"]);
}

fn flow_with_steps(count: usize) -> String {
    format!(
        "local flow = Flow.new('wide')\nfor index = 1, {count} do flow:step('s' .. index, nodes.log({{ message = 'x' }})) end\nreturn flow"
    )
}

#[test]
fn a_flow_holds_the_step_cap_and_rejects_one_more() {
    let flow =
        LuaRuntime::load_flow_from_string(&flow_with_steps(MAX_FLOW_STEPS), &registry()).unwrap();
    assert_eq!(flow.steps.len(), MAX_FLOW_STEPS);
    assert_eq!(flow.steps[MAX_FLOW_STEPS - 1].name, "s10000");

    let error = load_error(&flow_with_steps(MAX_FLOW_STEPS + 1));
    assert!(
        error.contains("10001 steps") && error.contains("limit is 10000"),
        "{error}"
    );
}

fn flow_with_dependencies(count: usize) -> String {
    format!(
        "local flow = Flow.new('fan_in')\nlocal join = flow:step('join', nodes.log({{ message = 'x' }}))\nfor index = 1, {count} do join:depends_on('dep' .. index) end\nreturn flow"
    )
}

#[test]
fn a_step_holds_the_dependency_cap_and_rejects_one_more() {
    let flow =
        LuaRuntime::load_flow_from_string(&flow_with_dependencies(MAX_FLOW_STEPS), &registry())
            .unwrap();
    assert_eq!(flow.steps[0].dependencies.len(), MAX_FLOW_STEPS);
    assert_eq!(flow.steps[0].dependencies[MAX_FLOW_STEPS - 1], "dep10000");

    let error = load_error(&flow_with_dependencies(MAX_FLOW_STEPS + 1));
    assert!(
        error.contains("Step 'join'")
            && error.contains("10001 dependencies")
            && error.contains("limit is 10000"),
        "{error}"
    );
}

#[test]
fn deep_top_level_expression_is_a_validation_error_on_a_worker_stack() {
    // 200,000 terms is 400 KB, inside the default 1 MiB flow-source limit.
    let source = flow_with_top_level_expression(200_000);
    let error = on_worker_stack(move || {
        let error = LuaRuntime::validate_flow_from_string(&source, &registry()).unwrap_err();
        format!("{error:#}")
    });
    assert!(
        error.contains("nested too deeply") && error.contains("1000 levels"),
        "{error}"
    );
}

#[test]
fn deep_embedded_code_source_is_a_validation_error_on_a_worker_stack() {
    let source = "local flow = Flow.new('deep_code')\nflow:step('calc', nodes.code({ source = 'return 1' .. string.rep('+1', 200000) }))\nreturn flow";
    let error = on_worker_stack(move || {
        let error = LuaRuntime::validate_flow_from_string(source, &registry()).unwrap_err();
        format!("{error:#}")
    });
    assert!(
        error.contains("Step 'calc'")
            && error.contains("nested too deeply")
            && error.contains("1000 levels"),
        "{error}"
    );
}

#[test]
fn nesting_that_lua_itself_accepts_validates_on_a_worker_stack() {
    // Lua stops at about 200 nested constructs and each is at most four
    // syntax-tree levels, so the analysis cap must not reject what Lua
    // compiles. Operator chains are the only construct Lua does not bound.
    let levels = 150;
    let cases = [
        format!("{}1{}", "(".repeat(levels), ")".repeat(levels)),
        format!("{}1{}", "{".repeat(levels), "}".repeat(levels)),
        format!("{}1{}", "tostring(".repeat(levels), ")".repeat(levels)),
        format!("'a'{}", "..'a'".repeat(levels)),
        format!(
            "{}1{}",
            "(function() return ".repeat(60),
            " end)()".repeat(60)
        ),
        chained_expression(900),
    ];
    for expression in cases {
        let source = format!(
            "local v = {expression}\nlocal flow = Flow.new('nested')\nflow:step('a', function()\n    return {{ value = {expression} }}\nend)\nreturn flow\n"
        );
        let validated = on_worker_stack(move || {
            LuaRuntime::validate_flow_from_string(&source, &registry())
                .map(|validated| validated.warnings)
                .map_err(|error| format!("{error:#}"))
        });
        assert_eq!(validated, Ok(Vec::new()));
    }
}

#[test]
fn loading_without_validation_does_not_analyze_the_syntax_tree() {
    // Only validation walks the syntax tree; Lua compiles operator chains
    // iteratively, so execution paths keep loading such a flow.
    let source = flow_with_top_level_expression(200_000);
    let flow = on_worker_stack(move || {
        LuaRuntime::load_flow_from_string(&source, &registry()).map(|flow| flow.name)
    });
    assert_eq!(flow.unwrap(), "deep");
}
