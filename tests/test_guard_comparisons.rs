//! Guard comparison semantics through `step_if` and `if_node` (IF-187).

use std::collections::HashMap;
use std::sync::Arc;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::*;
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::null_store::NullStateStore;
use serde_json::json;

const RUN: bool = true;
const SKIP: bool = false;

/// Comparisons with `true`, `false` and `nil` that routed "false" before the fix.
const LITERAL_GUARDS: &[(&str, bool)] = &[
    ("ctx.ready == true", RUN),
    ("ctx.ready != false", RUN),
    ("ctx.off == false", RUN),
    ("ctx.missing == nil", RUN),
    ("ctx.nothing == nil", RUN),
    ("ctx.ready != nil", RUN),
    ("ctx.obj != nil", RUN),
    ("ctx.list != nil", RUN),
    ("ctx.ready != 1", RUN),
    ("ctx.n != true", RUN),
    ("ctx.n != 'abc'", RUN),
    ("ctx.nothing != 5", RUN),
    // An operator inside the right operand belongs to the string.
    ("ctx.s != 'a==b'", RUN),
    ("ctx.expr == 'a==b'", RUN),
    ("ctx.expr == 'n>=1'", SKIP),
    ("ctx.expr != 'a==b'", SKIP),
    ("ctx.ready == false", SKIP),
    ("ctx.ready != true", SKIP),
    ("ctx.off != false", SKIP),
    ("ctx.off == nil", SKIP),
    ("ctx.missing != nil", SKIP),
    ("ctx.obj == nil", SKIP),
    ("ctx.ready == 1", SKIP),
    ("ctx.n == true", SKIP),
];

/// Results that were already correct and must stay exactly as they were.
const UNCHANGED_GUARDS: &[(&str, bool)] = &[
    ("ctx.ready", RUN),
    ("ctx.s", RUN),
    ("ctx.obj", RUN),
    ("ctx.off", SKIP),
    ("ctx.missing", SKIP),
    ("ctx.nothing", SKIP),
    ("ctx.nothing exists", RUN),
    ("ctx.missing exists", SKIP),
    ("ctx.n == 1", RUN),
    ("ctx.n == 1.0", RUN),
    ("ctx.n != 2", RUN),
    ("ctx.n > 0", RUN),
    ("ctx.n >= 1", RUN),
    ("ctx.n <= 1", RUN),
    ("ctx.n < 1", SKIP),
    ("ctx.n != 1", SKIP),
    ("ctx.nested.depth > 2", RUN),
    ("ctx.s == 'yes'", RUN),
    ("ctx.s == \\\"yes\\\"", RUN),
    ("ctx.s == yes", RUN),
    ("ctx.s != 'no'", RUN),
    ("ctx.s == 'no'", SKIP),
    ("ctx.s != 'yes'", SKIP),
    ("s == 'yes'", RUN),
    ("ctx.empty == ''", RUN),
    ("ctx.spaced == 'two words'", RUN),
    ("ctx.quoted == \\\"it's\\\"", RUN),
    ("ctx.s == and", SKIP),
    // A string value is compared with the operand's text.
    ("ctx.five == 5", RUN),
    ("ctx.five == 5.0", SKIP),
    ("ctx.flag_text == true", RUN),
    // A number never equals a quoted string.
    ("ctx.n == '1'", SKIP),
    ("ctx.missing == 1", SKIP),
    ("ctx.missing != 1", RUN),
];

/// Comparisons that cannot be decided before the value is known and now fail the guard.
const RUNTIME_ERRORS: &[&str] = &[
    "ctx.obj == 'x'",
    "ctx.obj != true",
    "ctx.list == 1",
    "ctx.ready > 0",
    "ctx.off <= 1",
    "ctx.obj >= 1",
    "ctx.list < 2",
    // Ordering needs a number in the context: absent values and strings fail too.
    "ctx.missing > 1",
    "ctx.nothing <= 1",
    "ctx.five > 3",
    "ctx.s < 1",
];

/// Comparisons whose right operand makes them invalid whatever the context holds.
const STATIC_ERRORS: &[&str] = &[
    "ctx.n > nil",
    "ctx.n <= true",
    "ctx.n < 'abc'",
    "ctx.n >= abc",
    "ctx.n > ctx.limit",
    // Syntax outside the grammar used to resolve to a missing path and route false.
    "not ctx.ready",
    "!ctx.ready",
    "(ctx.n > 0)",
    "ctx.ready and ctx.off",
    "ctx.n == 1 and ctx.s == 'yes'",
    "ctx.s == 'yes' or ctx.s == 'no'",
    "ctx.s ~= 'no'",
    "ctx.n = 1",
    "ctx.s == hello world",
    "ctx.s == 'it's'",
    "ctx.s == 'yes",
    "ctx.s ==",
    "== 1",
    "ctx..n == 1",
    "ctx.s == it exists",
    // A literal where a context path belongs used to be looked up as a key.
    "true",
    "false",
    "nil",
    "1",
    "true == true",
    "1 == 1",
    "0.5 > 0",
    "nil exists",
];

fn context() -> Context {
    HashMap::from([
        ("ready".to_string(), json!(true)),
        ("off".to_string(), json!(false)),
        ("n".to_string(), json!(1)),
        ("s".to_string(), json!("yes")),
        ("five".to_string(), json!("5")),
        ("expr".to_string(), json!("a==b")),
        ("empty".to_string(), json!("")),
        ("spaced".to_string(), json!("two words")),
        ("quoted".to_string(), json!("it's")),
        ("flag_text".to_string(), json!("true")),
        ("nothing".to_string(), json!(null)),
        ("obj".to_string(), json!({ "a": 1 })),
        ("list".to_string(), json!([1])),
        ("nested".to_string(), json!({ "depth": 3 })),
    ])
}

fn step_if_flow(condition: &str) -> String {
    format!(
        r#"
        local flow = Flow.new("guard_step_if")
        flow:step_if("{condition}", "action", nodes.code({{ source = "return {{ ran = true }}" }}))
        return flow
    "#
    )
}

fn if_node_flow(condition: &str) -> String {
    format!(
        r#"
        local flow = Flow.new("guard_if_node")
        flow:step("_if_action", nodes.if_node({{ condition = "{condition}" }}))
        flow:step("action", nodes.code({{ source = "return {{ ran = true }}" }}))
            :depends_on("_if_action"):route("true")
        return flow
    "#
    )
}

type FlowBuilder = fn(&str) -> String;

const BUILDERS: &[(&str, FlowBuilder)] = &[("step_if", step_if_flow), ("if_node", if_node_flow)];

async fn run(source: &str) -> RunInfo {
    let registry = Arc::new(NodeRegistry::with_builtins());
    let flow = LuaRuntime::load_flow_from_string(source, &registry).unwrap();
    let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let run_id = engine.execute(&flow, context()).await.unwrap();
    store.get_run_info(&run_id).await.unwrap()
}

async fn assert_routes(cases: &[(&str, bool)]) {
    let mut wrong = Vec::new();
    for (form, build) in BUILDERS {
        for (condition, expected) in cases {
            let info = run(&build(condition)).await;
            let ran = info.tasks["action"].status == TaskStatus::Success;
            let skipped = info.tasks["action"].status == TaskStatus::Skipped;
            let result = info.ctx.get("_condition_result__if_action");
            if info.status != RunStatus::Success
                || ran != *expected
                || skipped == *expected
                || result != Some(&json!(*expected))
                || info.ctx.contains_key("ran") != *expected
            {
                wrong.push(format!(
                    "{form} `{condition}`: expected run={expected}, run status {:?}, action {:?}, guard error {:?}",
                    info.status, info.tasks["action"].status, info.tasks["_if_action"].error
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "wrong guard results:\n{}",
        wrong.join("\n")
    );
}

#[tokio::test]
async fn boolean_and_nil_literals_compare_at_runtime() {
    assert_routes(LITERAL_GUARDS).await;
}

#[tokio::test]
async fn existing_comparisons_keep_their_results() {
    assert_routes(UNCHANGED_GUARDS).await;
}

#[tokio::test]
async fn unsupported_comparisons_fail_the_guard_instead_of_routing_false() {
    let mut wrong = Vec::new();
    for (form, build) in BUILDERS {
        for condition in RUNTIME_ERRORS {
            let info = run(&build(condition)).await;
            let guard = &info.tasks["_if_action"];
            let error = guard.error.clone().unwrap_or_default();
            if info.status != RunStatus::Failed
                || guard.status != TaskStatus::Failed
                || info.tasks["action"].status != TaskStatus::Skipped
                || !error.contains(condition)
                || info.ctx.contains_key("_route__if_action")
            {
                wrong.push(format!(
                    "{form} `{condition}`: run {:?}, guard {:?}, action {:?}, error {error:?}",
                    info.status, guard.status, info.tasks["action"].status
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "guards that did not fail:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn statically_invalid_comparisons_fail_flow_loading() {
    let registry = NodeRegistry::with_builtins();
    let mut wrong = Vec::new();
    for (form, build) in BUILDERS {
        for condition in STATIC_ERRORS {
            match LuaRuntime::load_flow_from_string(&build(condition), &registry) {
                Ok(_) => wrong.push(format!("{form} `{condition}` loaded")),
                Err(error) => {
                    let message = format!("{error:#}");
                    if !message.contains(condition) || !message.contains("_if_action") {
                        wrong.push(format!("{form} `{condition}`: unclear error {message:?}"));
                    }
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "invalid conditions accepted:\n{}",
        wrong.join("\n")
    );
}

#[tokio::test]
async fn unsupported_comparison_error_does_not_echo_the_context_value() {
    let source = r#"
        local flow = Flow.new("guard_secret")
        flow:step("seed", nodes.code({ source = "return { profile = { token = 'hunter2-secret' } }" }))
        flow:step_if("ctx.profile == 'x'", "action", nodes.log({ message = "no" }))
            :depends_on("seed")
        return flow
    "#;
    let info = run(source).await;
    let error = info.tasks["_if_action"].error.clone().unwrap();
    assert!(error.contains("ctx.profile == 'x'"), "{error}");
    assert!(error.contains("object"), "{error}");
    assert!(!error.contains("hunter2"), "{error}");
}

#[tokio::test]
async fn guard_error_quotes_a_condition_on_a_credential_named_key_unchanged() {
    for condition in ["ctx.credentials == 'x'", "ctx.api_token=='x'"] {
        let source = format!(
            r#"
            local flow = Flow.new("guard_sensitive_key")
            flow:step("seed", nodes.code({{ source = "return {{ credentials = {{ id = 'hunter2-secret' }}, api_token = {{ id = 'hunter2-secret' }} }}" }}))
            flow:step_if("{condition}", "action", nodes.log({{ message = "no" }}))
                :depends_on("seed")
            return flow
        "#
        );
        let info = run(&source).await;
        let error = info.tasks["_if_action"].error.clone().unwrap();
        assert!(error.contains(condition), "{error}");
        assert!(!error.contains("REDACTED"), "{error}");
        assert!(!error.contains("hunter2"), "{error}");
    }
}
