// IF-058: the JSON/Lua conversion ceilings are configurable like every other
// IRONFLOW_MAX_* limit. Dedicated test binary because it mutates a
// process-global environment variable.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

/// Build a Lua table deep enough to exceed a lowered depth ceiling.
fn nested_source(depth: usize) -> String {
    let mut s = String::from("local v = 1\n");
    for _ in 0..depth {
        s.push_str("v = { inner = v }\n");
    }
    s.push_str("return { deep = v }\n");
    s
}

#[tokio::test]
async fn conversion_depth_ceiling_is_configurable_and_names_its_variable() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("code").unwrap();
    let config = serde_json::json!({ "source": nested_source(40) });

    // Well under the 64 default: succeeds.
    unsafe { std::env::remove_var("IRONFLOW_MAX_CONVERSION_DEPTH") };
    assert!(
        node.execute(&config, &Context::new()).await.is_ok(),
        "40 levels is within the default ceiling"
    );

    // Lowered below the payload: rejected, and the error names the override.
    unsafe { std::env::set_var("IRONFLOW_MAX_CONVERSION_DEPTH", "10") };
    let error = node
        .execute(&config, &Context::new())
        .await
        .expect_err("40 levels must exceed a ceiling of 10")
        .to_string();
    unsafe { std::env::remove_var("IRONFLOW_MAX_CONVERSION_DEPTH") };

    assert!(error.contains("maximum depth"), "{error}");
    assert!(
        error.contains("IRONFLOW_MAX_CONVERSION_DEPTH"),
        "error must name the variable that raises it: {error}"
    );
}

#[tokio::test]
async fn conversion_node_ceiling_is_configurable_and_names_its_variable() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("code").unwrap();
    // A flat list of 500 values: far under the 100k default, over a cap of 50.
    let config = serde_json::json!({
        "source": "local t = {} for i = 1, 500 do t[i] = i end return { items = t }"
    });

    unsafe { std::env::remove_var("IRONFLOW_MAX_CONVERSION_NODES") };
    assert!(
        node.execute(&config, &Context::new()).await.is_ok(),
        "500 values is within the default ceiling"
    );

    unsafe { std::env::set_var("IRONFLOW_MAX_CONVERSION_NODES", "50") };
    let error = node
        .execute(&config, &Context::new())
        .await
        .expect_err("500 values must exceed a ceiling of 50")
        .to_string();
    unsafe { std::env::remove_var("IRONFLOW_MAX_CONVERSION_NODES") };

    assert!(error.contains("maximum node count"), "{error}");
    assert!(
        error.contains("IRONFLOW_MAX_CONVERSION_NODES"),
        "error must name the variable that raises it: {error}"
    );
}
