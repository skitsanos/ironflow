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

/// Resolve one reference as a required mapping entry.
fn resolve(reference: &str, parent: &Context, call: &Value) -> Result<Value, InputMappingError> {
    resolve_spec("scope", "entry", &json!(reference), parent, call)
}

fn call() -> Value {
    json!({"id": "call-1", "name": "inspect", "index": 0,
           "arguments": {"items": [{"id": 8}], "city": "Berlin", "nothing": null}})
}

#[test]
fn missing_call_selectors_are_null_without_root_fallback() {
    let parent = parent();
    let call = call();
    for reference in [
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
            resolve(reference, &parent, &call),
            Ok(Value::Null),
            "{reference}"
        );
    }
}

#[test]
fn missing_and_malformed_parent_selectors_fail_and_are_null_only_when_optional() {
    let parent = parent();
    let call = call();
    for reference in [
        "ctx.user.id",
        "ctx.user.profile.id",
        "ctx.user.profile.name.id",
        "ctx.absent.id",
        "ctx.absent",
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
        "absent",
        "a literal",
    ] {
        assert_eq!(
            resolve(reference, &parent, &call),
            Err(InputMappingError::MissingSource {
                scope: "scope".into(),
                entry: "entry".into()
            }),
            "{reference}"
        );
        let mapping = json!({"input": {"entry": reference}});
        let tools = json!({"inspect": mapping});
        let checked = validate_inputs(
            tools.as_object().unwrap(),
            std::slice::from_ref(&call),
            &parent,
        );
        assert!(checked.is_err(), "{reference}");
        let optional = json!({ "optional": reference });
        assert_eq!(
            resolve_spec("scope", "entry", &optional, &parent, &call),
            Ok(Value::Null),
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
            resolve(&format!("ctx.values.{index}"), &parent, &call()),
            Ok(expected.clone())
        );
    }
    for (reference, expected) in [
        ("ctx.user.profile.name", json!("Ada")),
        ("ctx.items.0.id", json!(7)),
        ("ctx.nothing", Value::Null),
        ("nothing", Value::Null),
        ("arguments.items.0.id", json!(8)),
        ("arguments.city", json!("Berlin")),
        ("call.arguments.items.0.id", json!(8)),
        ("call.index", json!(0)),
        ("arguments.nothing", Value::Null),
    ] {
        assert_eq!(
            resolve(reference, &parent, &call()),
            Ok(expected),
            "{reference}"
        );
    }
}

#[test]
fn explicit_root_references_and_bare_parent_keys_select_whole_values() {
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
    ] {
        assert_eq!(
            resolve(reference, &parent, &call),
            Ok(expected),
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
        "inspect",
        &json!({"input": {"payload": {
            "name": "ctx.user.profile.name", "id": {"optional": "ctx.user.id"},
            "nested": [{"id": {"optional": "ctx.items.0.absent"}}, "arguments.items.0.id"],
            "literal": {"literal": "a literal"}, "values": [false, 0, null]
        }}}),
        &parent,
        &call,
    )
    .unwrap();
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
    let child = build_child_context("inspect", &json!({}), &parent(), &call).unwrap();
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

#[test]
fn a_nested_failure_names_the_entry_path_and_only_called_tools_are_checked() {
    let parent = parent();
    let tools = json!({
        "inspect": {"input": {"payload": {"list": [1, {"id": "ctx.user.id"}]}}},
        "unused": {"input": {"value": "ctx.absent"}},
        "plain": {"input": {"value": "ctx.user"}}
    });
    let tools = tools.as_object().unwrap();
    let mut other = call();
    other["name"] = json!("plain");
    assert_eq!(validate_inputs(tools, &[other.clone()], &parent), Ok(()));
    let error = validate_inputs(tools, &[other, call()], &parent).unwrap_err();
    assert_eq!(
        error.to_string(),
        "tool_dispatch: tool 'inspect': input 'payload.list.1.id' references a parent context \
         value that does not exist; use { optional = \"<key>\" } when it may be absent or \
         { literal = <value> } to pass a literal"
    );
}
