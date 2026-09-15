use mlua::prelude::*;
use serde_json::json;

use super::{ConversionLimits, JsonToLuaConverter};
use crate::engine::types::Context;
use crate::lua::context_keys::project_context;

fn convert(ctx: &Context, max_nodes: usize, max_depth: usize) -> anyhow::Result<LuaValue> {
    let lua = Lua::new();
    JsonToLuaConverter::new(
        &lua,
        ConversionLimits {
            max_nodes,
            max_depth,
        },
    )
    .convert_context(ctx)
}

#[test]
fn context_projection_budget_counts_root_and_all_selected_keys() {
    let ctx = Context::from([
        ("a".into(), json!([1, 2])),
        ("b".into(), json!([3, 4])),
        ("unused".into(), json!(vec![0; 200_000])),
    ]);
    let selected = project_context(&json!({"context_keys": ["b", "a", "a"]}), &ctx).unwrap();
    assert_eq!(selected.len(), 2);
    assert!(convert(&selected, 7, 2).is_ok());
    let error = convert(&selected, 6, 2).unwrap_err().to_string();
    assert!(error.contains("maximum node count 6"), "{error}");
    assert!(error.contains("$.b[1]"), "{error}");
}

#[test]
fn context_projection_keeps_original_root_depth_and_paths() {
    let ctx = Context::from([("a.b".into(), json!({"nested": [1]}))]);
    let selected = project_context(&json!({"context_keys": ["a.b"]}), &ctx).unwrap();
    assert!(convert(&selected, 4, 3).is_ok());
    let error = convert(&selected, 4, 2).unwrap_err().to_string();
    assert!(error.contains("maximum depth 2"), "{error}");
    assert!(error.contains("$[\"a.b\"].nested[0]"), "{error}");
}

#[test]
fn context_projection_excludes_large_values_but_still_bounds_selected_values() {
    let ctx = Context::from([("unused".into(), json!(vec![0; 200_000]))]);
    let empty = project_context(&json!({"context_keys": []}), &ctx).unwrap();
    assert!(convert(&empty, 1, 0).is_ok());
    assert!(convert(&empty, 0, 0).is_err());

    for config in [json!({}), json!({"context_keys": ["unused"]})] {
        let selected = project_context(&config, &ctx).unwrap();
        let error = convert(&selected, 100_000, 64).unwrap_err().to_string();
        assert!(error.contains("maximum node count 100000"), "{error}");
        assert!(error.contains("$.unused[99998]"), "{error}");
    }
}

#[test]
fn context_projection_matches_whole_json_object_conversion() {
    let ctx = Context::from([
        ("a".into(), json!({"items": [1, null]})),
        ("b".into(), json!([])),
    ]);
    let object = json!(ctx);
    let lua = Lua::new();
    for max_nodes in 0..=8 {
        for max_depth in 0..=4 {
            let limits = ConversionLimits {
                max_nodes,
                max_depth,
            };
            let context_result = JsonToLuaConverter::new(&lua, limits).convert_context(&ctx);
            let object_result = JsonToLuaConverter::new(&lua, limits).convert(&object, "$", 0);
            assert_eq!(context_result.is_ok(), object_result.is_ok());
            if let (Err(context_error), Err(object_error)) = (context_result, object_result) {
                assert_eq!(context_error.to_string(), object_error.to_string());
            }
        }
    }
}

#[test]
fn context_projection_keeps_engine_diagnostics_but_not_bulky_engine_payloads() {
    let ctx = Context::from([
        ("order_id".into(), json!(7)),
        ("unused".into(), json!([1, 2, 3])),
        ("_error_message".into(), json!("boom")),
        ("_error_step".into(), json!("risky")),
        ("_error_node_type".into(), json!("code")),
        ("_flow_dir".into(), json!("/flows")),
        ("_error_output".into(), json!({"rows": [1, 2, 3]})),
        ("_headers".into(), json!({"authorization": "Bearer x"})),
    ]);
    // The scalar recovery diagnostics and `_flow_dir` are always present;
    // bulky engine payloads stay behind the allowlist like user data.
    let selected = project_context(&json!({"context_keys": ["order_id"]}), &ctx).unwrap();
    assert_eq!(selected.len(), 5);
    assert_eq!(selected["order_id"], 7);
    assert_eq!(selected["_error_message"], "boom");
    assert_eq!(selected["_error_step"], "risky");
    assert_eq!(selected["_error_node_type"], "code");
    assert_eq!(selected["_flow_dir"], "/flows");
    assert!(!selected.contains_key("unused"));
    assert!(!selected.contains_key("_error_output"));
    assert!(!selected.contains_key("_headers"));

    // Bulky engine payloads are selectable by listing them explicitly.
    let explicit = project_context(
        &json!({"context_keys": ["_error_output", "_headers"]}),
        &ctx,
    )
    .unwrap();
    assert_eq!(explicit.len(), 6);
    assert_eq!(explicit["_error_output"]["rows"], json!([1, 2, 3]));
    assert!(explicit.contains_key("_headers"));

    // An empty list hides every user key and every bulky payload.
    let empty = project_context(&json!({"context_keys": []}), &ctx).unwrap();
    assert_eq!(empty.len(), 4);
    assert!(
        empty
            .keys()
            .all(|key| key.starts_with("_error_") || key == "_flow_dir")
    );
    assert!(!empty.contains_key("_error_output"));

    // Listing an always-present key explicitly is harmless and selects it once.
    let repeated = project_context(&json!({"context_keys": ["_flow_dir"]}), &ctx).unwrap();
    assert_eq!(repeated.len(), 4);

    // Omitting the projection is unchanged: the whole context is cloned.
    assert_eq!(project_context(&json!({}), &ctx).unwrap().len(), 8);
}
