use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;

use ironflow::engine::types::Context;

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn foreach_config(source: &str) -> serde_json::Value {
    let reg = NodeRegistry::with_builtins();
    let flow = LuaRuntime::load_flow_from_string(source, &reg).unwrap();
    flow.steps[0].config.clone()
}

#[tokio::test]
async fn foreach_collects_transformed_results() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("foreach").unwrap();

    let config = foreach_config(
        r#"
        local flow = Flow.new("foreach_collect")
        flow:step("x", nodes.foreach({
            source_key = "items",
            output_key = "mapped",
            transform = function(item, idx)
                return { name = item.name, index = idx }
            end
        }))
        return flow
    "#,
    );

    let ctx = ctx_with(vec![(
        "items",
        serde_json::json!([
            { "name": "alpha" },
            { "name": "beta" },
            { "name": "gamma" }
        ]),
    )]);

    let out = node.execute(&config, &ctx).await.unwrap();
    let arr = out.get("mapped").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].get("name").unwrap(), "alpha");
    assert_eq!(arr[0].get("index").unwrap(), 1);
    assert_eq!(out.get("mapped_count").unwrap(), 3);
}

#[tokio::test]
async fn foreach_filter_nulls_defaults_to_true() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("foreach").unwrap();

    let config = foreach_config(
        r#"
        local flow = Flow.new("foreach_filter")
        flow:step("x", nodes.foreach({
            source_key = "items",
            output_key = "active_items",
            filter_nulls = true,
            transform = function(item)
                if item.flag == true then
                    return { value = item.value }
                end
            end
        }))
        return flow
    "#,
    );

    let ctx = ctx_with(vec![(
        "items",
        serde_json::json!([
            {"value": "a", "flag": true},
            {"value": "b", "flag": false},
            {"value": "c", "flag": true}
        ]),
    )]);

    let out = node.execute(&config, &ctx).await.unwrap();
    let arr = out.get("active_items").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(out.get("active_items_count").unwrap(), 2);
}

#[tokio::test]
async fn foreach_filter_nulls_false_preserves_nil_results() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("foreach").unwrap();

    let config = foreach_config(
        r#"
        local flow = Flow.new("foreach_preserve")
        flow:step("x", nodes.foreach({
            source_key = "items",
            output_key = "mapped",
            filter_nulls = false,
            transform = function(item)
                if item > 1 then
                    return item
                end
            end
        }))
        return flow
    "#,
    );

    let ctx = ctx_with(vec![("items", serde_json::json!([0, 1, 2, 3]))]);

    let out = node.execute(&config, &ctx).await.unwrap();
    let arr = out.get("mapped").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 4);
    assert_eq!(arr[0], serde_json::Value::Null);
    assert_eq!(arr[1], serde_json::Value::Null);
    assert_eq!(out.get("mapped_count").unwrap(), 4);
}

#[tokio::test]
async fn foreach_does_not_expose_package_loader_or_system_libraries() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("foreach").unwrap();

    let config = foreach_config(
        r#"
        local flow = Flow.new("foreach_sandbox")
        flow:step("x", nodes.foreach({
            source_key = "items",
            output_key = "sandbox_types",
            transform = function()
                return {
                    require_type = type(require),
                    package_type = type(package),
                    os_type = type(os),
                    io_type = type(io),
                    load_type = type(load),
                    collectgarbage_type = type(collectgarbage),
                    string_dump_type = type(string.dump)
                }
            end
        }))
        return flow
    "#,
    );

    let ctx = ctx_with(vec![("items", serde_json::json!([1]))]);
    let out = node.execute(&config, &ctx).await.unwrap();
    let result = &out["sandbox_types"][0];

    for key in [
        "require_type",
        "package_type",
        "os_type",
        "io_type",
        "load_type",
        "collectgarbage_type",
        "string_dump_type",
    ] {
        assert_eq!(result.get(key), Some(&serde_json::json!("nil")), "{key}");
    }
}
