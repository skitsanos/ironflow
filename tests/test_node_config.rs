//! Numeric node config values must accept the same forms string params already do:
//! a native JSON number, a numeric string, or a `${ctx.*}` template that resolves to
//! either. Before these helpers existed, `config.get(k).and_then(|v| v.as_f64())`
//! returned `None` for every string form, so an interpolated parameter was silently
//! replaced by the node's default.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::util::node_config::{config_bool, config_f64, config_u64};

fn ctx_with(key: &str, value: serde_json::Value) -> Context {
    let mut ctx = HashMap::new();
    ctx.insert(key.to_string(), value);
    ctx
}

#[test]
fn f64_reads_a_native_json_number() {
    let config = serde_json::json!({ "threshold": 0.3 });
    assert_eq!(config_f64(&config, "threshold", &HashMap::new()), Some(0.3));
}

#[test]
fn f64_reads_a_numeric_string() {
    let config = serde_json::json!({ "threshold": "0.3" });
    assert_eq!(config_f64(&config, "threshold", &HashMap::new()), Some(0.3));
}

#[test]
fn f64_resolves_an_interpolated_number_from_context() {
    let ctx = ctx_with("threshold", serde_json::json!(0.3));
    let config = serde_json::json!({ "threshold": "${ctx.threshold}" });
    assert_eq!(config_f64(&config, "threshold", &ctx), Some(0.3));
}

#[test]
fn f64_resolves_an_interpolated_numeric_string_from_context() {
    let ctx = ctx_with("threshold", serde_json::json!("0.75"));
    let config = serde_json::json!({ "threshold": "${ctx.threshold}" });
    assert_eq!(config_f64(&config, "threshold", &ctx), Some(0.75));
}

#[test]
fn f64_is_none_when_key_is_absent() {
    let config = serde_json::json!({});
    assert_eq!(config_f64(&config, "threshold", &HashMap::new()), None);
}

#[test]
fn f64_is_none_when_value_is_not_numeric() {
    let config = serde_json::json!({ "threshold": "high" });
    assert_eq!(config_f64(&config, "threshold", &HashMap::new()), None);
}

#[test]
fn f64_is_none_when_interpolation_resolves_to_nothing() {
    let config = serde_json::json!({ "threshold": "${ctx.missing}" });
    assert_eq!(config_f64(&config, "threshold", &HashMap::new()), None);
}

#[test]
fn u64_reads_a_native_json_number() {
    let config = serde_json::json!({ "sg_window": 15 });
    assert_eq!(config_u64(&config, "sg_window", &HashMap::new()), Some(15));
}

#[test]
fn u64_reads_a_numeric_string() {
    let config = serde_json::json!({ "sg_window": "15" });
    assert_eq!(config_u64(&config, "sg_window", &HashMap::new()), Some(15));
}

#[test]
fn u64_resolves_an_interpolated_number_from_context() {
    let ctx = ctx_with("window", serde_json::json!(15));
    let config = serde_json::json!({ "sg_window": "${ctx.window}" });
    assert_eq!(config_u64(&config, "sg_window", &ctx), Some(15));
}

#[test]
fn u64_accepts_a_whole_number_written_as_a_float() {
    // Lua has one number type, so an integer parameter routinely arrives as 15.0.
    let config = serde_json::json!({ "sg_window": 15.0 });
    assert_eq!(config_u64(&config, "sg_window", &HashMap::new()), Some(15));
}

#[test]
fn u64_is_none_for_a_negative_value() {
    let config = serde_json::json!({ "sg_window": -3 });
    assert_eq!(config_u64(&config, "sg_window", &HashMap::new()), None);
}

#[test]
fn u64_is_none_when_value_is_not_numeric() {
    let config = serde_json::json!({ "sg_window": "wide" });
    assert_eq!(config_u64(&config, "sg_window", &HashMap::new()), None);
}

#[test]
fn bool_reads_a_native_json_bool() {
    let config = serde_json::json!({ "pretty": true });
    assert_eq!(config_bool(&config, "pretty", &HashMap::new()), Some(true));
}

#[test]
fn bool_reads_the_accepted_truthy_and_falsy_words() {
    // Same vocabulary the S3 helpers already accept for boolean env vars.
    for word in ["true", "TRUE", "yes", "on", "1"] {
        let config = serde_json::json!({ "pretty": word });
        assert_eq!(
            config_bool(&config, "pretty", &HashMap::new()),
            Some(true),
            "expected {word:?} to read as true"
        );
    }
    for word in ["false", "False", "no", "off", "0"] {
        let config = serde_json::json!({ "pretty": word });
        assert_eq!(
            config_bool(&config, "pretty", &HashMap::new()),
            Some(false),
            "expected {word:?} to read as false"
        );
    }
}

#[test]
fn bool_resolves_an_interpolated_bool_from_context() {
    let ctx = ctx_with("pretty", serde_json::json!(true));
    let config = serde_json::json!({ "pretty": "${ctx.pretty}" });
    assert_eq!(config_bool(&config, "pretty", &ctx), Some(true));
}

#[test]
fn bool_resolves_an_interpolated_string_from_context() {
    let ctx = ctx_with("flag", serde_json::json!("no"));
    let config = serde_json::json!({ "pretty": "${ctx.flag}" });
    assert_eq!(config_bool(&config, "pretty", &ctx), Some(false));
}

#[test]
fn bool_is_none_when_key_is_absent() {
    let config = serde_json::json!({});
    assert_eq!(config_bool(&config, "pretty", &HashMap::new()), None);
}

#[test]
fn bool_is_none_when_value_is_not_boolean() {
    let config = serde_json::json!({ "pretty": "maybe" });
    assert_eq!(config_bool(&config, "pretty", &HashMap::new()), None);
}
