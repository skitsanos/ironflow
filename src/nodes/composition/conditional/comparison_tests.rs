use serde_json::{Value, json};

use super::*;

fn evaluate(condition: &str, left: Option<&Value>) -> Result<bool, ComparisonError> {
    Comparison::parse(condition)
        .unwrap()
        .expect("a comparison")
        .evaluate(left)
}

fn values() -> Vec<Option<Value>> {
    vec![
        None,
        Some(Value::Null),
        Some(json!(true)),
        Some(json!(false)),
        Some(json!(0)),
        Some(json!(1)),
        Some(json!(1.5)),
        Some(json!("")),
        Some(json!("1")),
        Some(json!("true")),
        Some(json!("nil")),
        Some(json!("yes")),
        Some(json!([1])),
        Some(json!({ "a": 1 })),
    ]
}

const OPERANDS: &[&str] = &[
    "nil",
    "true",
    "false",
    "0",
    "1",
    "1.0",
    "1.5",
    "-2",
    "yes",
    "'yes'",
    "\"1\"",
    "'true'",
    "'nil'",
    "''",
    "\"two words\"",
];

#[test]
fn not_equal_is_the_negation_of_equal_for_every_operand_pair() {
    for left in values() {
        for operand in OPERANDS {
            let equal = evaluate(&format!("ctx.v == {operand}"), left.as_ref());
            let not_equal = evaluate(&format!("ctx.v != {operand}"), left.as_ref());
            match (equal, not_equal) {
                (Ok(equal), Ok(not_equal)) => {
                    assert_ne!(equal, not_equal, "{left:?} against {operand}")
                }
                (Err(equal), Err(not_equal)) => assert_eq!(equal, not_equal),
                other => panic!("{left:?} against {operand}: {other:?}"),
            }
        }
    }
}

#[test]
fn only_containers_compared_with_a_non_nil_operand_have_no_equality() {
    for left in values() {
        let container = left.as_ref().is_some_and(|v| v.is_array() || v.is_object());
        for operand in OPERANDS {
            let result = evaluate(&format!("ctx.v == {operand}"), left.as_ref());
            assert_eq!(
                result.is_err(),
                container && *operand != "nil",
                "{left:?} against {operand}"
            );
        }
    }
}

#[test]
fn absent_and_null_equal_nil_only() {
    for left in [None, Some(Value::Null)] {
        assert_eq!(evaluate("ctx.v == nil", left.as_ref()), Ok(true));
        assert_eq!(evaluate("ctx.v == 'nil'", left.as_ref()), Ok(false));
        assert_eq!(evaluate("ctx.v == false", left.as_ref()), Ok(false));
        assert_eq!(evaluate("ctx.v == 0", left.as_ref()), Ok(false));
    }
}

#[test]
fn booleans_equal_boolean_literals_only() {
    let ready = json!(true);
    assert_eq!(evaluate("ctx.v == true", Some(&ready)), Ok(true));
    assert_eq!(evaluate("ctx.v == false", Some(&ready)), Ok(false));
    assert_eq!(evaluate("ctx.v == 'true'", Some(&ready)), Ok(false));
    assert_eq!(evaluate("ctx.v == 1", Some(&ready)), Ok(false));
    assert_eq!(evaluate("ctx.v == nil", Some(&ready)), Ok(false));
}

#[test]
fn strings_compare_with_the_operand_text() {
    let text = json!("true");
    assert_eq!(evaluate("ctx.v == true", Some(&text)), Ok(true));
    assert_eq!(evaluate("ctx.v == 'true'", Some(&text)), Ok(true));
    assert_eq!(evaluate("ctx.v == 5", Some(&json!("5"))), Ok(true));
    assert_eq!(evaluate("ctx.v == 5.0", Some(&json!("5"))), Ok(false));
}

#[test]
fn ordering_needs_a_number_on_both_sides() {
    assert_eq!(evaluate("ctx.v >= 1", Some(&json!(1))), Ok(true));
    assert_eq!(evaluate("ctx.v < 1", Some(&json!(1))), Ok(false));
    for (left, kind) in [
        (None, "nil"),
        (Some(Value::Null), "nil"),
        (Some(json!("5")), "a string"),
        (Some(json!(true)), "a boolean"),
        (Some(json!([1])), "an array"),
        (Some(json!({})), "an object"),
    ] {
        assert_eq!(
            evaluate("ctx.v > 1", left.as_ref()),
            Err(ComparisonError::OrderingValue {
                operator: ">",
                path: "ctx.v".to_string(),
                kind,
            })
        );
    }
}

#[test]
fn ordering_against_a_non_numeric_literal_is_rejected_at_parse() {
    for (condition, operator, found) in [
        ("ctx.v > nil", ">", "nil"),
        ("ctx.v >= true", ">=", "true"),
        ("ctx.v < 'abc'", "<", "'abc'"),
        ("ctx.v <= ctx.limit", "<=", "ctx.limit"),
    ] {
        assert_eq!(
            Comparison::parse(condition).unwrap_err(),
            ComparisonError::OrderingLiteral {
                operator,
                found: found.to_string(),
            }
        );
    }
}

#[test]
fn a_condition_without_an_operator_is_not_a_comparison() {
    assert!(Comparison::parse("ctx.ready").unwrap().is_none());
    assert!(Comparison::parse("ctx.token exists").unwrap().is_none());
}

#[test]
fn error_messages_name_the_operand_and_type_only() {
    let secret = json!({ "token": "hunter2" });
    let message = evaluate("ctx.credentials == 'x'", Some(&secret))
        .unwrap_err()
        .to_string();
    assert_eq!(
        message,
        "ctx.credentials holds an object, which can only be compared with nil"
    );
}

#[test]
fn the_leftmost_operator_splits_the_condition() {
    let text = json!("a==b");
    assert_eq!(evaluate("ctx.v != 'a==b'", Some(&text)), Ok(false));
    assert_eq!(evaluate("ctx.v != 'a==b'", Some(&json!("yes"))), Ok(true));
    assert_eq!(evaluate("ctx.v == 'a==b'", Some(&text)), Ok(true));
    assert_eq!(
        evaluate("ctx.v != \"x<=y\"", Some(&json!("x<=y"))),
        Ok(false)
    );
    assert_eq!(evaluate("ctx.v == 'a>b'", Some(&json!("a>b"))), Ok(true));
    assert_eq!(evaluate("ctx.v != a==b", Some(&text)), Ok(false));
    for (condition, left) in [
        ("ctx.v>=1", "ctx.v"),
        ("ctx.v <= 1", "ctx.v"),
        ("ctx.v != 'a==b'", "ctx.v"),
        ("ctx.v == '!='", "ctx.v"),
    ] {
        assert_eq!(Comparison::parse(condition).unwrap().unwrap().left, left);
    }
    assert_eq!(evaluate("ctx.v>=1", Some(&json!(1))), Ok(true));
    assert_eq!(evaluate("ctx.v<=0", Some(&json!(1))), Ok(false));
}

#[test]
fn operands_outside_the_grammar_are_rejected_at_parse() {
    for (condition, found) in [
        ("ctx.v == 1 and ctx.w == 2", "1 and ctx.w == 2"),
        ("ctx.v == 'a' or ctx.w == 'b'", "'a' or ctx.w == 'b'"),
        ("ctx.v == hello world", "hello world"),
        ("ctx.v == 'it's'", "'it's'"),
        ("ctx.v == 'open", "'open"),
        ("ctx.v == open'", "open'"),
        ("ctx.v == '", "'"),
        ("ctx.v ==", ""),
        ("ctx.v >", ""),
    ] {
        assert_eq!(
            Comparison::parse(condition).unwrap_err(),
            ComparisonError::Operand {
                found: found.to_string(),
            },
            "{condition}"
        );
    }
    for (condition, found) in [
        ("(ctx.v > 0)", "(ctx.v"),
        ("not ctx.v == 1", "not ctx.v"),
        ("== 1", ""),
        ("ctx..v == 1", "ctx..v"),
        ("ctx. == 1", "ctx."),
    ] {
        assert_eq!(
            Comparison::parse(condition).unwrap_err(),
            ComparisonError::Path {
                found: found.to_string(),
            },
            "{condition}"
        );
    }
}

#[test]
fn paths_have_no_spaces_quotes_or_operator_characters() {
    assert_eq!(context_path("ctx.user.age"), Ok("user.age"));
    assert_eq!(context_path("amount"), Ok("amount"));
    assert_eq!(
        context_path("ctx._headers.x-tenant-token"),
        Ok("_headers.x-tenant-token")
    );
    for path in [
        "not ctx.v",
        "!ctx.v",
        "ctx.a and ctx.b",
        "ctx.v = 1",
        "ctx.v ~",
        "(ctx.v)",
        "'ctx.v'",
        "ctx.a||ctx.b",
        "ctx.",
        "",
        "ctx.a..b",
    ] {
        assert!(context_path(path).is_err(), "{path}");
    }
}

#[test]
fn quoted_operands_keep_their_exact_text() {
    assert_eq!(
        evaluate("ctx.v == 'two words'", Some(&json!("two words"))),
        Ok(true)
    );
    assert_eq!(
        evaluate("ctx.v == \"it's\"", Some(&json!("it's"))),
        Ok(true)
    );
    assert_eq!(evaluate("ctx.v == ''", Some(&json!(""))), Ok(true));
    assert_eq!(
        evaluate("ctx.v == '\"x\"'", Some(&json!("\"x\""))),
        Ok(true)
    );
    assert_eq!(evaluate("ctx.v == and", Some(&json!("and"))), Ok(true));
}

#[test]
fn a_literal_is_not_a_context_path() {
    for literal in ["true", "false", "nil", "1", "0.5", "-2", "+3", ".5", "1e3"] {
        assert_eq!(
            context_path(literal),
            Err(ComparisonError::LiteralPath {
                found: literal.to_string(),
            })
        );
    }
    for (condition, found) in [
        ("true == true", "true"),
        ("1 == 1", "1"),
        ("0.5 > 0", "0.5"),
    ] {
        assert_eq!(
            Comparison::parse(condition).unwrap_err(),
            ComparisonError::LiteralPath {
                found: found.to_string(),
            }
        );
    }
    // The prefix still reaches keys with these names, and words are paths.
    assert_eq!(context_path("ctx.true"), Ok("true"));
    assert_eq!(context_path("ctx.1"), Ok("1"));
    assert_eq!(context_path("truthy"), Ok("truthy"));
    assert_eq!(context_path("infinity"), Ok("infinity"));
    assert_eq!(context_path("items.0"), Ok("items.0"));
}
