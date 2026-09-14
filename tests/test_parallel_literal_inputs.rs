use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

fn fixture(directory: &Path, item_key: &str, index_key: &str) -> Context {
    std::fs::write(directory.join("echo.lua"), format!(
        "local flow = Flow.new('literal-child')\nflow:step('echo', function(ctx) return {{received = ctx[{}], position = ctx[{}]}} end)\nreturn flow",
        json!(item_key), json!(index_key)
    )).unwrap();
    Context::from([
        ("_flow_dir".into(), json!(directory)),
        ("customer".into(), json!({"id": "parent-customer"})),
        ("alias".into(), json!("customer")),
        ("".into(), json!("not-empty")),
        ("parent_only".into(), json!("not-inherited")),
    ])
}

async fn execute(config: &Value, ctx: &Context) -> Context {
    NodeRegistry::with_builtins()
        .get("parallel_subworkflows")
        .unwrap()
        .execute(config, ctx)
        .await
        .unwrap()
}

#[tokio::test]
async fn dynamic_items_preserve_every_json_type_without_resolving_parent_keys() {
    let items = json!([
        "customer", "", "jobs", "_flow_dir", "ordinary-literal", "${ctx.customer}",
        {"id": "customer"}, ["customer", null, false], [], {}, null, false, true,
        42, -7, 1.25, i64::MAX
    ]);
    for (item_key, index_key) in [("item", "index"), ("_batch", "_ordinal")] {
        for concurrency in [1, 4] {
            let directory = tempfile::tempdir().unwrap();
            let mut ctx = fixture(directory.path(), item_key, index_key);
            ctx.insert("jobs".into(), items.clone());
            let before = ctx.clone();
            let config = json!({
                "flow": "echo.lua", "source_key": "jobs", "item_key": item_key,
                "index_key": index_key, "child_output_key": "child", "max_concurrent": concurrency,
                "input": {"mapped": "customer", "mapped_once": "alias", "literal": "unmatched", "nested": {"key": "customer"}}
            });
            let output = execute(&config, &ctx).await;
            assert_eq!(output["parallel_results_all_succeeded"], true);
            assert_eq!(
                output["parallel_results_count"],
                items.as_array().unwrap().len()
            );
            for (index, item) in items.as_array().unwrap().iter().enumerate() {
                let child = &output["parallel_results"][index]["child"];
                assert_eq!(&child["received"], item, "item {index} changed");
                assert_eq!(child["position"], index + 1);
                assert_eq!(child["mapped"], ctx["customer"]);
                assert_eq!(child["mapped_once"], "customer");
                assert_eq!(child["literal"], "unmatched");
                assert_eq!(child["nested"], json!({"key": "customer"}));
                assert!(child.get("parent_only").is_none());
                assert!(child.get("_flow_dir").is_none());
            }
            assert_eq!(ctx, before);
        }
    }
}

#[tokio::test]
async fn literal_item_and_index_override_same_named_authored_mappings() {
    let directory = tempfile::tempdir().unwrap();
    let mut ctx = fixture(directory.path(), "job", "ordinal");
    ctx.insert("jobs".into(), json!(["customer"]));
    let output = execute(&json!({
        "flow": "echo.lua", "source_key": "jobs", "item_key": "job", "index_key": "ordinal",
        "child_output_key": "child", "input": {"job": "alias", "ordinal": "customer", "mapped": "customer"}
    }), &ctx).await;
    let child = &output["parallel_results"][0]["child"];
    assert_eq!(child["received"], "customer");
    assert_eq!(child["position"], 1);
    assert_eq!(child["mapped"], ctx["customer"]);
}

#[tokio::test]
async fn source_name_as_item_key_does_not_turn_literal_into_the_source_array() {
    let directory = tempfile::tempdir().unwrap();
    let mut ctx = fixture(directory.path(), "jobs", "ordinal");
    ctx.insert("jobs".into(), json!(["jobs"]));
    let output = execute(
        &json!({
            "flow": "echo.lua", "source_key": "jobs", "item_key": "jobs", "index_key": "ordinal"
        }),
        &ctx,
    )
    .await;
    assert_eq!(output["parallel_results"][0]["received"], "jobs");
    assert_eq!(output["parallel_results"][0]["position"], 1);
    assert!(output["parallel_results"][0].get("parent_only").is_none());
}

#[tokio::test]
async fn static_mapping_keeps_reference_literal_and_parent_inheritance_contracts() {
    let directory = tempfile::tempdir().unwrap();
    let ctx = fixture(directory.path(), "item", "index");
    let output = execute(&json!({"flows": [
        {"flow": "echo.lua", "output_key": "child"},
        {"flow": "echo.lua", "output_key": "child", "input": {"item": "customer", "mapped_once": "alias", "literal": "unmatched", "nested": ["customer"]}},
        {"flow": "echo.lua", "output_key": "child", "input": {}}
    ]}), &ctx).await;
    let inherited = &output["parallel_results"][0]["child"];
    assert_eq!(inherited["parent_only"], ctx["parent_only"]);
    let mapped = &output["parallel_results"][1]["child"];
    assert_eq!(mapped["received"], ctx["customer"]);
    assert_eq!(mapped["mapped_once"], "customer");
    assert_eq!(mapped["literal"], "unmatched");
    assert_eq!(mapped["nested"], json!(["customer"]));
    assert!(mapped.get("parent_only").is_none());
    assert_eq!(output["parallel_results"][2]["child"], json!({}));
}

#[tokio::test]
async fn same_item_and_index_key_keeps_existing_index_precedence() {
    let directory = tempfile::tempdir().unwrap();
    let mut ctx = fixture(directory.path(), "slot", "slot");
    ctx.insert("jobs".into(), json!(["customer", null]));
    let output = execute(
        &json!({
            "flow": "echo.lua", "source_key": "jobs", "item_key": "slot", "index_key": "slot"
        }),
        &ctx,
    )
    .await;
    let results = output["parallel_results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    for (index, result) in results.iter().enumerate() {
        assert_eq!(result["received"], index + 1);
        assert_eq!(result["position"], index + 1);
    }
}
