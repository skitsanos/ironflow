//! IF-223 (a): a number that the JSON value model or a Lua number cannot
//! hold exactly is kept as text or fails the step; it is never rounded or
//! replaced by `null`.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

async fn run(node: &str, config: Value, ctx: &Context) -> anyhow::Result<Context> {
    let registry = NodeRegistry::with_builtins();
    let output = registry.get(node).unwrap().execute(&config, ctx).await?;
    Ok(output.into_iter().collect())
}

fn ctx_with(key: &str, value: Value) -> Context {
    [(key.to_string(), value)].into_iter().collect()
}

async fn yaml(text: &str) -> anyhow::Result<Value> {
    let output = run(
        "yaml_parse",
        json!({"source_key": "text"}),
        &ctx_with("text", json!(text)),
    )
    .await?;
    Ok(output["yaml_data"].clone())
}

async fn json_node(text: &str) -> anyhow::Result<Value> {
    let output = run(
        "json_parse",
        json!({"source_key": "text", "output_key": "out"}),
        &ctx_with("text", json!(text)),
    )
    .await?;
    Ok(output["out"].clone())
}

// --- yaml_parse ---

#[tokio::test]
async fn yaml_non_finite_numbers_stay_text() {
    let parsed = yaml("a: .nan\nb: .inf\nc: -.Inf\nd: +.inf\ne: 1e999\nf: [.NaN, -1.0e+400]\n")
        .await
        .unwrap();
    assert_eq!(
        parsed,
        json!({
            "a": ".nan", "b": ".inf", "c": "-.Inf", "d": "+.inf", "e": "1e999",
            "f": [".NaN", "-1.0e+400"]
        })
    );
}

#[tokio::test]
async fn yaml_integers_outside_the_signed_64_bit_range_stay_text() {
    let parsed = yaml(
        "over: 9223372036854775808\nunder: -9223372036854775809\n\
         id: 12345678901234567890\nplus: +18446744073709551615\n\
         wide: 123456789012345678901234567890\n",
    )
    .await
    .unwrap();
    assert_eq!(
        parsed,
        json!({
            "over": "9223372036854775808",
            "under": "-9223372036854775809",
            "id": "12345678901234567890",
            "plus": "+18446744073709551615",
            "wide": "123456789012345678901234567890"
        })
    );
}

#[tokio::test]
async fn yaml_text_is_kept_through_aliases_merges_and_tags() {
    let parsed = yaml(
        "base: &base {big: 99999999999999999999, small: 1}\n\
         merged:\n  <<: *base\n  small: 2\n\
         list: [&id 99999999999999999999, *id]\n\
         tagged: !custom 99999999999999999999\n\
         float: !!float 99999999999999999999\n",
    )
    .await
    .unwrap();
    assert_eq!(
        parsed,
        json!({
            "base": {"big": "99999999999999999999", "small": 1},
            "merged": {"small": 2, "big": "99999999999999999999"},
            "list": ["99999999999999999999", "99999999999999999999"],
            "tagged": "99999999999999999999",
            "float": 1e20
        })
    );
}

#[tokio::test]
async fn yaml_explicit_non_finite_float_fails_the_step() {
    let error = yaml("value: !!float .inf\n").await.unwrap_err().to_string();
    assert!(error.contains("yaml_parse"), "{error}");
    assert!(error.contains("not a finite number"), "{error}");
}

#[tokio::test]
async fn yaml_representable_scalars_are_unchanged() {
    let parsed = yaml(
        "max: 9223372036854775807\nmin: -9223372036854775808\nzero: 0\nlead: 0123\n\
         float: 1e30\nwhole: 1.0e19\ndotted: 12345678901234567890.0\nratio: 0.1875\n\
         quoted: \"12345678901234567890\"\nword: inf\nyes: true\nnone: ~\n\
         hex: 0xFFFFFFFFFFFFFFFFFF\n",
    )
    .await
    .unwrap();
    assert_eq!(
        parsed,
        json!({
            "max": 9223372036854775807i64,
            "min": -9223372036854775808i64,
            "zero": 0,
            "lead": 123,
            "float": 1e30,
            "whole": 1.0e19,
            "dotted": 1.2345678901234567e19,
            "ratio": 0.1875,
            "quoted": "12345678901234567890",
            "word": "inf",
            "yes": true,
            "none": null,
            "hex": "0xFFFFFFFFFFFFFFFFFF"
        })
    );
    assert!(parsed["max"].is_i64() && parsed["lead"].is_i64());
    assert!(parsed["float"].is_f64() && parsed["whole"].is_f64());
}

// --- yaml_stringify ---

#[tokio::test]
async fn yaml_stringify_keeps_number_like_text_as_text() {
    let source = json!({
        "text": [".nan", ".inf", "-.Inf", "1e999", "12345678901234567890", "~", "true"],
        "float": 1e30,
        "integer": 9223372036854775807i64
    });
    let written = run(
        "yaml_stringify",
        json!({"source_key": "doc"}),
        &ctx_with("doc", source.clone()),
    )
    .await
    .unwrap();
    assert_eq!(
        yaml(written["yaml"].as_str().unwrap()).await.unwrap(),
        source
    );
}

#[tokio::test]
async fn yaml_stringify_refuses_an_integer_it_cannot_write_exactly() {
    let error = run(
        "yaml_stringify",
        json!({"source_key": "doc"}),
        &ctx_with("doc", json!({"n": 18446744073709551615u64})),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("yaml_stringify"), "{error}");
    assert!(error.contains("18446744073709551615"), "{error}");
    assert!(!error.contains("Cargo"), "{error}");
}

// --- json_parse and json_extract_path ---

#[tokio::test]
async fn json_integers_outside_the_value_model_stay_text() {
    let parsed = json_node(
        r#"{"wide": 123456789012345678901234567890, "over": 18446744073709551616,
            "under": -9223372036854775809, "nested": [[-99999999999999999999]]}"#,
    )
    .await
    .unwrap();
    assert_eq!(
        parsed,
        json!({
            "wide": "123456789012345678901234567890",
            "over": "18446744073709551616",
            "under": "-9223372036854775809",
            "nested": [["-99999999999999999999"]]
        })
    );
}

#[tokio::test]
async fn json_representable_numbers_and_strings_are_unchanged() {
    let text = r#"{"max": 18446744073709551615, "min": -9223372036854775808,
        "float": 1e30, "dotted": 123456789012345678901234567890.0, "small": -0.5e-3,
        "text": "id 123456789012345678901234567890 \" 99999999999999999999 \\",
        "123456789012345678901234567890": [0, -1, true, null]}"#;
    let expected: Value = serde_json::from_str(text).unwrap();
    let parsed = json_node(text).await.unwrap();
    assert_eq!(parsed, expected);
    assert!(parsed["max"].is_u64() && parsed["min"].is_i64());
    assert!(parsed["float"].is_f64() && parsed["dotted"].is_f64());
}

#[tokio::test]
async fn json_errors_are_reported_as_before() {
    for text in [
        "[1e999]",
        "[NaN]",
        "[123456789012345678901234567890, }",
        "[123456789012345678901234567890 123]",
        "[-]",
        "\"open 123456789012345678901234567890",
    ] {
        let expected = serde_json::from_str::<Value>(text).unwrap_err().to_string();
        let error = json_node(text).await.unwrap_err().to_string();
        assert_eq!(error, expected, "{text}");
    }
}

#[tokio::test]
async fn json_extract_path_parses_wide_integers_as_text() {
    let ctx = ctx_with("text", json!(r#"{"id": 123456789012345678901234567890}"#));
    let output = run(
        "json_extract_path",
        json!({"source_key": "text", "path": "id", "output_key": "id", "parse_json": true}),
        &ctx,
    )
    .await
    .unwrap();
    assert_eq!(output["id"], json!("123456789012345678901234567890"));
}

// --- xml_parse ---

#[tokio::test]
async fn xml_parse_never_converts_text_to_numbers() {
    let ctx = ctx_with(
        "xml",
        json!("<r n=\"1e999\"><a>12345678901234567890</a><b>NaN</b><c>007</c></r>"),
    );
    let output = run("xml_parse", json!({"source_key": "xml"}), &ctx)
        .await
        .unwrap();
    assert_eq!(
        output["xml_data"],
        json!({"r": {"@n": "1e999", "a": "12345678901234567890", "b": "NaN", "c": "007"}})
    );
}
