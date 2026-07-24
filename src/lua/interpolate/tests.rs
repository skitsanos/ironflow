use std::collections::HashMap;

use serde_json::json;

use super::*;

fn context() -> Context {
    HashMap::from([
        (
            "user".to_string(),
            json!({"name": "Alice", "contact.info": {"email": "alice@example.com"}}),
        ),
        (
            "items".to_string(),
            json!([{"name": "first"}, {"name": "second"}]),
        ),
        ("matrix".to_string(), json!([[1, 2], [3, 4]])),
        ("amount".to_string(), json!(42)),
        ("enabled".to_string(), json!(true)),
        ("nothing".to_string(), Value::Null),
    ])
}

#[test]
fn interpolates_object_and_zero_based_array_paths() {
    let ctx = context();

    assert_eq!(
        try_interpolate_ctx("${ctx.user.name}: ${ctx.items[0].name}", &ctx).unwrap(),
        "Alice: first"
    );
    assert_eq!(
        try_interpolate_ctx("${ctx.matrix[1][0]}", &ctx).unwrap(),
        "3"
    );
}

#[test]
fn interpolates_json_quoted_object_keys() {
    assert_eq!(
        try_interpolate_ctx("${ctx.user[\"contact.info\"].email}", &context()).unwrap(),
        "alice@example.com"
    );
}

#[test]
fn preserves_legacy_missing_and_null_behavior() {
    let ctx = context();
    assert_eq!(
        try_interpolate_ctx("missing=${ctx.missing}; null=${ctx.nothing}", &ctx).unwrap(),
        "missing=; null="
    );
    assert_eq!(
        try_interpolate_ctx("${ctx.items[99].name}", &ctx).unwrap(),
        ""
    );
}

#[test]
fn stringifies_non_string_json_values() {
    let ctx = context();
    assert_eq!(
        try_interpolate_ctx("${ctx.amount}/${ctx.enabled}/${ctx.items}", &ctx).unwrap(),
        "42/true/[{\"name\":\"first\"},{\"name\":\"second\"}]"
    );
}

#[test]
fn leaves_foreign_expansions_untouched() {
    assert_eq!(
        try_interpolate_ctx("${HOME}:${TMPDIR:-/tmp}:${name}", &context()).unwrap(),
        "${HOME}:${TMPDIR:-/tmp}:${name}"
    );
}

#[test]
fn escape_is_distinct_from_currency_prefix() {
    let ctx = context();
    assert_eq!(
        try_interpolate_ctx(r"\${ctx.user.name}", &ctx).unwrap(),
        "${ctx.user.name}"
    );
    assert_eq!(try_interpolate_ctx("$${ctx.amount}", &ctx).unwrap(), "$42");
    assert_eq!(try_interpolate_ctx(r"\${HOME}", &ctx).unwrap(), r"\${HOME}");
}

#[test]
fn interpolation_is_one_pass() {
    let ctx = HashMap::from([
        ("first".to_string(), json!("${ctx.second}")),
        ("second".to_string(), json!("expanded")),
    ]);
    assert_eq!(
        try_interpolate_ctx("value=${ctx.first}", &ctx).unwrap(),
        "value=${ctx.second}"
    );
}

#[test]
fn invalid_reserved_expressions_are_rejected() {
    let invalid = [
        "${ctx}",
        "${ctx.}",
        "${ctx..name}",
        "${ctx.items[-1]}",
        "${ctx.items[1.5]}",
        "${ctx.items[]}",
        "${ctx.items[01]}",
        "${ctx.name or env(\"NAME\")}",
        "${ctx.items[0]",
    ];

    for template in invalid {
        assert!(
            try_interpolate_ctx(template, &context()).is_err(),
            "expected `{template}` to be rejected"
        );
    }
}

#[test]
fn compatibility_wrapper_preserves_invalid_template() {
    let template = "${ctx.name or 'unknown'}";
    assert_eq!(interpolate_ctx(template, &context()), template);
}

#[test]
fn recursive_interpolation_ignores_object_keys() {
    let value = json!({
        "${ctx.user.name}": "structural key",
        "nested": ["${ctx.items[0].name}", {"amount": "$${ctx.amount}"}]
    });
    assert_eq!(
        interpolate_value(&value, &context()),
        json!({
            "${ctx.user.name}": "structural key",
            "nested": ["first", {"amount": "$42"}]
        })
    );
}

#[test]
fn recursive_validation_reports_value_paths() {
    let value = json!({
        "headers": {"Authorization": "Bearer ${ctx.token or env(\"TOKEN\")}"},
        "items": ["ok", "${ctx.values[-1]}"]
    });
    let paths: Vec<_> = validate_value(&value)
        .into_iter()
        .map(|(path, _)| path)
        .collect();

    assert_eq!(
        paths,
        vec!["config.headers.Authorization", "config.items[1]"]
    );
}
