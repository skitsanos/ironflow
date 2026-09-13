use serde_json::json;

use super::*;

fn parent() -> Context {
    Context::from([
        (
            "user".into(),
            json!({"profile": {"name": "Ada"}, "private": "private-sibling"}),
        ),
        (
            "items".into(),
            json!([{"id": 7, "private": "item-private"}]),
        ),
        ("scalar".into(), json!(42)),
        ("nothing".into(), Value::Null),
        ("".into(), json!({"user": "empty-key-private"})),
    ])
}

fn call() -> Value {
    json!({"id": "call-1", "name": "inspect", "index": 0,
           "arguments": {"items": [{"id": 8}], "city": "Berlin", "nothing": null}})
}

#[test]
fn missing_and_malformed_selectors_are_null_without_root_fallback() {
    let parent = parent();
    let call = call();
    for reference in [
        "ctx.user.id",
        "ctx.user.profile.id",
        "ctx.user.profile.name.id",
        "ctx.absent.id",
        "ctx.items.1.id",
        "ctx.items.0.absent",
        "ctx.items.-1",
        "ctx.items.999999999999999999999999999999999999",
        "ctx.scalar.id",
        "ctx.nothing.id",
        "ctx.",
        "ctx..user",
        "ctx.user.",
        "ctx.user..profile",
        "ctx.user.profile.",
        "arguments.",
        "arguments..city",
        "arguments.items.",
        "arguments.items.1",
        "arguments.items.0.absent",
        "arguments.city.id",
        "arguments.nothing.id",
        "arguments.absent",
        "call.",
        "call..name",
        "call.arguments.",
        "call.arguments..city",
        "call.name.id",
        "call.absent",
    ] {
        assert_eq!(
            resolve_string(reference, &parent, &call),
            Value::Null,
            "{reference}"
        );
    }
}

#[test]
fn complete_nested_paths_preserve_types_and_array_positions() {
    let mut parent = parent();
    let values = json!([0, false, null, "", {}, [], "ctx.user.private"]);
    parent.insert("values".into(), values.clone());
    for (index, expected) in values.as_array().unwrap().iter().enumerate() {
        assert_eq!(
            resolve_string(&format!("ctx.values.{index}"), &parent, &call()),
            *expected
        );
    }
    for (reference, expected) in [
        ("ctx.user.profile.name", json!("Ada")),
        ("ctx.items.0.id", json!(7)),
        ("arguments.items.0.id", json!(8)),
        ("arguments.city", json!("Berlin")),
        ("call.arguments.items.0.id", json!(8)),
        ("call.index", json!(0)),
        ("arguments.nothing", Value::Null),
    ] {
        assert_eq!(
            resolve_string(reference, &parent, &call()),
            expected,
            "{reference}"
        );
    }
}

#[test]
fn explicit_root_references_and_legacy_key_or_literal_strings_are_unchanged() {
    let parent = parent();
    let call = call();
    for (reference, expected) in [
        ("ctx.user", parent["user"].clone()),
        ("user", parent["user"].clone()),
        ("ctx.items", parent["items"].clone()),
        ("call", call.clone()),
        ("arguments", call["arguments"].clone()),
        ("tool_name", json!("inspect")),
        ("tool_call_id", json!("call-1")),
        ("a literal", json!("a literal")),
        ("absent", json!("absent")),
    ] {
        assert_eq!(
            resolve_string(reference, &parent, &call),
            expected,
            "{reference}"
        );
    }
}

#[test]
fn recursive_input_mapping_does_not_copy_unselected_siblings() {
    let parent = parent();
    let original = parent.clone();
    let call = call();
    let child = build_child_context(
        &json!({"input": {"payload": {
            "name": "ctx.user.profile.name", "id": "ctx.user.id",
            "nested": [{"id": "ctx.items.0.absent"}, "arguments.items.0.id"],
            "literal": "a literal", "values": [false, 0, null]
        }}}),
        &parent,
        &call,
    );
    assert_eq!(
        child["payload"],
        json!({"name": "Ada", "id": null,
        "nested": [{"id": null}, 8], "literal": "a literal", "values": [false, 0, null]})
    );
    assert!(!serde_json::to_string(&child).unwrap().contains("private"));
    assert_eq!(parent, original);
}

#[test]
fn default_child_context_contains_call_metadata_but_not_parent_context() {
    let call = call();
    let child = build_child_context(&json!({}), &parent(), &call);
    assert_eq!(
        child,
        Context::from([
            ("tool_call".into(), call.clone()),
            ("tool_name".into(), call["name"].clone()),
            ("tool_call_id".into(), call["id"].clone()),
            ("tool_arguments".into(), call["arguments"].clone()),
            ("tool_call_index".into(), call["index"].clone()),
        ])
    );
}
