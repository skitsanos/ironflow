//! IF-189: `json_parse` builds Lua values while it reads the text, inside the
//! conversion budgets and the Lua memory limit.

use mlua::prelude::*;
use serde_json::json;

use super::{ConversionLimits, json_parse, lua_value_to_json, register_json_globals};

const LIMITS: ConversionLimits = ConversionLimits {
    max_depth: 64,
    max_nodes: 100_000,
};

fn lua() -> Lua {
    let lua = Lua::new();
    register_json_globals(&lua).unwrap();
    lua
}

fn parse_error(lua: &Lua, text: &str, limits: ConversionLimits) -> String {
    json_parse::parse(lua, text, limits)
        .unwrap_err()
        .to_string()
}

/// A flat array of `elements` ones: `[1,1,...,1]`.
fn flat_array(elements: usize) -> String {
    format!("[{}1]", "1,".repeat(elements - 1))
}

#[test]
fn ordinary_documents_convert_to_the_same_values_as_before() {
    let lua = lua();
    let documents = [
        r#"null"#,
        r#"true"#,
        r#"-0"#,
        r#"12"#,
        r#"-9223372036854775808"#,
        r#"9223372036854775807"#,
        r#"1.5e3"#,
        r#""plain""#,
        r#""escaped \" \\ \n é 😀""#,
        r#"[]"#,
        r#"{}"#,
        r#"[1, "two", null, [], {}, [[3]], {"k": [null]}]"#,
        r#"{"a": 1, "b": {"c": [true, false]}, "not an identifier": null, "": 0}"#,
        r#"{"repeated": 1, "repeated": [2]}"#,
        "  \n[ 1 ,\t2 ]\n",
    ];

    for document in documents {
        let expected: serde_json::Value = serde_json::from_str(document).unwrap();
        let value = json_parse::parse(&lua, document, LIMITS).unwrap();
        assert_eq!(
            lua_value_to_json(&lua, &value).unwrap(),
            expected,
            "{document}"
        );
    }
}

#[test]
fn integers_floats_null_and_empty_containers_keep_their_lua_shape() {
    let lua = lua();
    let check = lua
        .load(
            r#"
            local value = json_parse('{"int": 7, "float": 7.0, "big": 18446744073709551615, '
                .. '"null": null, "array": [], "object": {}, "list": [null, 2]}')
            assert(math.type(value.int) == "integer")
            assert(math.type(value.float) == "float")
            assert(math.type(value.big) == "float")
            assert(value.null == json_null and value.null ~= nil)
            assert(json_stringify(value.array) == "[]")
            assert(json_stringify(value.object) == "{}")
            assert(value.list[1] == json_null and value.list[2] == 2 and #value.list == 2)
            -- A number is coerced to text, as before.
            assert(json_parse(5) == 5)
            return true
            "#,
        )
        .eval::<bool>();
    assert!(check.unwrap());
}

#[test]
fn syntax_errors_keep_the_parser_message() {
    let lua = lua();
    for (text, expected) in [
        ("", "EOF while parsing a value at line 1 column 0"),
        ("[1,", "EOF while parsing a value at line 1 column 3"),
        ("{\"a\" 1}", "expected `:` at line 1 column 6"),
        ("[1] x", "trailing characters at line 1 column 5"),
        ("{1: 2}", "key must be a string at line 1 column 2"),
        ("1e999", "number out of range at line 1 column 5"),
    ] {
        assert_eq!(
            parse_error(&lua, text, LIMITS),
            format!("runtime error: json_parse failed: {expected}"),
            "{text}"
        );
    }
}

#[test]
fn the_node_budget_names_the_first_value_over_it() {
    let lua = lua();
    let limits = ConversionLimits {
        max_nodes: 5,
        ..LIMITS
    };

    // The array and four elements fit; the fifth element is the sixth value.
    assert!(json_parse::parse(&lua, "[1,2,3,4]", limits).is_ok());
    let error = parse_error(&lua, "[1,2,3,4,5]", limits);
    assert!(
        error.contains("JSON-to-Lua maximum node count 5 exceeded at $[4]")
            && error.contains("IRONFLOW_MAX_CONVERSION_NODES"),
        "{error}"
    );
    let error = parse_error(&lua, r#"{"a": [1, 2], "b c": {"d": 1}}"#, limits);
    assert!(error.contains(r#"exceeded at $["b c"].d"#), "{error}");
}

#[test]
fn the_depth_budget_names_the_first_value_over_it() {
    let lua = lua();
    let limits = ConversionLimits {
        max_depth: 2,
        ..LIMITS
    };

    assert!(json_parse::parse(&lua, r#"{"a": [1]}"#, limits).is_ok());
    let error = parse_error(&lua, r#"{"a": [[1]]}"#, limits);
    assert!(
        error.contains("JSON-to-Lua maximum depth 2 exceeded at $.a[0][0]")
            && error.contains("IRONFLOW_MAX_CONVERSION_DEPTH"),
        "{error}"
    );
}

#[test]
fn a_syntax_error_after_an_exceeded_budget_is_still_reported_first() {
    let lua = lua();
    let limits = ConversionLimits {
        max_nodes: 2,
        ..LIMITS
    };

    let error = parse_error(&lua, "[1,2,3,4", limits);

    assert!(error.contains("EOF while parsing a list"), "{error}");
}

#[test]
fn the_storage_nesting_limit_is_reported_ahead_of_the_conversion_budget() {
    let lua = lua();
    let nested = |depth: usize| format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let tight = ConversionLimits {
        max_nodes: 3,
        ..LIMITS
    };

    for limits in [LIMITS, tight] {
        let error = parse_error(&lua, &nested(124), limits);
        assert!(
            error.contains("json_parse failed: JSON value is nested deeper than 123 levels"),
            "{error}"
        );
    }
    let error = parse_error(&lua, &nested(123), LIMITS);
    assert!(error.contains("IRONFLOW_MAX_CONVERSION_DEPTH"), "{error}");

    // With the conversion depth raised, text at the storage limit is admitted.
    let deep = ConversionLimits {
        max_depth: 200,
        ..LIMITS
    };
    assert!(json_parse::parse(&lua, &nested(123), deep).is_ok());
    assert!(parse_error(&lua, &nested(124), deep).contains("nested deeper than 123 levels"));
}

/// The reviewed input: an array far over the node budget. Before the fix the
/// whole tree was built outside Lua and the array table was then sized for
/// every element, so the call ended in a Lua memory error after hundreds of
/// megabytes. Now it stops building at the budget.
#[test]
fn an_array_far_over_the_node_budget_is_refused_without_being_built() {
    let lua = lua();
    lua.set_memory_limit(32 * 1024 * 1024).unwrap();
    let text = flat_array(5_000_000);
    let before = lua.used_memory();

    let error = lua
        .load("return json_parse(...)")
        .call::<LuaValue>(text.as_str())
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("JSON-to-Lua maximum node count 100000 exceeded at $[99999]"),
        "{error}"
    );
    lua.gc_collect().unwrap();
    let retained = lua.used_memory().saturating_sub(before);
    assert!(retained < 1024 * 1024, "retained {retained} bytes");
}

/// With the node budget out of the way, the Lua memory limit bounds the
/// tables: construction stops there and the limit is what the caller sees.
#[test]
fn the_lua_memory_limit_bounds_what_a_raised_node_budget_admits() {
    let lua = lua();
    lua.set_memory_limit(8 * 1024 * 1024).unwrap();
    let limits = ConversionLimits {
        max_nodes: usize::MAX,
        ..LIMITS
    };
    let text = flat_array(2_000_000);

    let error = parse_error(&lua, &text, limits);

    assert!(error.contains("not enough memory"), "{error}");
    assert!(lua.used_memory() <= 8 * 1024 * 1024);
    // The state is usable afterwards and ordinary text still parses.
    lua.gc_collect().unwrap();
    let value = json_parse::parse(&lua, "[1,2,3]", LIMITS).unwrap();
    assert_eq!(lua_value_to_json(&lua, &value).unwrap(), json!([1, 2, 3]));
}

#[test]
fn text_that_is_not_utf8_is_refused() {
    let lua = lua();
    let error = lua
        .load(r#"return json_parse("[\"\xff\"]")"#)
        .eval::<LuaValue>()
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("utf-8") || error.contains("UTF-8"),
        "{error}"
    );
}
