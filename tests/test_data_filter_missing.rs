//! IF-223 (c): in `data_filter`, `neq` is the exact negation of `eq`, a
//! missing field compares as `null`, and a comparison without `value` fails.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

async fn run(node: &str, mut config: Value, items: Value) -> anyhow::Result<Context> {
    let config_map = config.as_object_mut().unwrap();
    config_map.insert("source_key".into(), json!("items"));
    config_map.insert("output_key".into(), json!("out"));
    let ctx: Context = [("items".to_string(), items)].into_iter().collect();
    let registry = NodeRegistry::with_builtins();
    let output = registry.get(node).unwrap().execute(&config, &ctx).await?;
    Ok(output.into_iter().collect())
}

async fn filter(items: &Value, op: &str, value: Option<Value>) -> anyhow::Result<Value> {
    let mut config = json!({"field": "status", "op": op});
    if let Some(value) = value {
        config["value"] = value;
    }
    Ok(run("data_filter", config, items.clone()).await?["out"].clone())
}

fn items() -> Value {
    json!([
        {"id": 1, "status": "active"},
        {"id": 2, "status": "closed"},
        {"id": 3},
        {"id": 4, "status": null},
        {"id": 5, "status": 7},
        "not an object"
    ])
}

fn ids(filtered: &Value) -> Vec<Value> {
    filtered
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.get("id").cloned().unwrap_or_else(|| item.clone()))
        .collect()
}

#[tokio::test]
async fn neq_matches_items_without_the_field() {
    let items = items();
    let equal = filter(&items, "eq", Some(json!("active"))).await.unwrap();
    let unequal = filter(&items, "neq", Some(json!("active"))).await.unwrap();
    assert_eq!(ids(&equal), [json!(1)]);
    assert_eq!(
        ids(&unequal),
        [
            json!(2),
            json!(3),
            json!(4),
            json!(5),
            json!("not an object")
        ]
    );
}

#[tokio::test]
async fn eq_and_neq_split_every_array_without_overlap() {
    let items = items();
    for value in [
        json!("active"),
        json!(7),
        json!(7.0),
        json!(null),
        json!("absent"),
        json!([1]),
    ] {
        let equal = filter(&items, "eq", Some(value.clone())).await.unwrap();
        let unequal = filter(&items, "neq", Some(value.clone())).await.unwrap();
        let mut all = ids(&equal);
        all.extend(ids(&unequal));
        all.sort_by_key(|id| id.to_string());
        let mut expected = ids(&items);
        expected.sort_by_key(|id| id.to_string());
        assert_eq!(all, expected, "{value}");
    }
}

#[tokio::test]
async fn a_missing_field_compares_as_null() {
    let items = items();
    let equal = filter(&items, "eq", Some(Value::Null)).await.unwrap();
    let unequal = filter(&items, "neq", Some(Value::Null)).await.unwrap();
    assert_eq!(ids(&equal), [json!(3), json!(4), json!("not an object")]);
    assert_eq!(ids(&unequal), [json!(1), json!(2), json!(5)]);
    // The same split as the presence operators.
    assert_eq!(equal, filter(&items, "not_exists", None).await.unwrap());
    assert_eq!(unequal, filter(&items, "exists", None).await.unwrap());
}

#[tokio::test]
async fn a_comparison_without_value_fails_the_step() {
    let items = items();
    for op in ["eq", "neq", "gt", "lt", "gte", "lte", "contains"] {
        let error = filter(&items, op, None).await.unwrap_err().to_string();
        assert!(error.contains("data_filter"), "{error}");
        assert!(error.contains(&format!("'{op}'")), "{error}");
        assert!(error.contains("'value'"), "{error}");
    }
}

#[tokio::test]
async fn ordering_and_contains_exclude_missing_and_mistyped_fields() {
    let items = items();
    for op in ["gt", "lt", "gte", "lte"] {
        let matched = filter(&items, op, Some(json!(7))).await.unwrap();
        let expected: &[Value] = if op == "gte" || op == "lte" {
            &[json!(5)]
        } else {
            &[]
        };
        assert_eq!(ids(&matched), expected, "{op}");
        let mistyped = filter(&items, op, Some(json!("7"))).await.unwrap();
        assert_eq!(ids(&mistyped), [] as [Value; 0], "{op}");
    }
    let contains = filter(&items, "contains", Some(json!("s"))).await.unwrap();
    assert_eq!(ids(&contains), [json!(2)]);
    let contains_number = filter(&items, "contains", Some(json!(7))).await.unwrap();
    assert_eq!(ids(&contains_number), [] as [Value; 0]);
}

#[tokio::test]
async fn deduplicate_keeps_one_item_without_the_key_and_one_with_null() {
    let items = json!([
        {"id": 1, "email": "a@example.com"},
        {"id": 2},
        {"id": 3, "email": null},
        {"id": 4},
        {"id": 5, "email": null},
        {"id": 6, "email": "a@example.com"}
    ]);
    let output = run("deduplicate", json!({"key": "email"}), items)
        .await
        .unwrap();
    assert_eq!(ids(&output["out"]), [json!(1), json!(2), json!(3)]);
    assert_eq!(output["out_removed"], json!(3));
}
