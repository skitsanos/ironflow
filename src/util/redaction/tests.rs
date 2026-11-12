use super::*;

#[test]
fn overlay_redactor_removes_keys_and_nested_secret_values() {
    let overlay = Context::from([(
        "_headers".to_string(),
        serde_json::json!({"stripe-signature": "t=12345678,v1=super-secret-value"}),
    )]);
    let redactor = SecretRedactor::from_overlay(&overlay);
    let context = Context::from([
        ("_headers".to_string(), overlay["_headers"].clone()),
        (
            "result".to_string(),
            serde_json::json!({
                "copy": "t=12345678,v1=super-secret-value",
                "component": "super-secret-value",
                "message": "signature: t=12345678,v1=super-secret-value",
                "super-secret-value": true
            }),
        ),
    ]);

    let redacted = redactor.redact_context(&context);

    assert!(!redacted.contains_key("_headers"));
    let serialized = serde_json::to_string(&redacted).unwrap();
    assert!(!serialized.contains("super-secret-value"));
    assert!(serialized.contains(REDACTED));
}

#[test]
fn owned_redaction_preserves_security_contract() {
    let secret = "owned-super-secret-value";
    let redactor = SecretRedactor::from_overlay(&Context::from([(
        "_headers".to_string(),
        serde_json::json!({"authorization": secret}),
    )]));
    let context = Context::from([
        (
            "_headers".to_string(),
            serde_json::json!({"authorization": secret}),
        ),
        (
            "result".to_string(),
            serde_json::json!({"copy": secret, "owned-super-secret-value": true}),
        ),
    ]);

    let redacted = redactor.redact_context_owned(context);
    let serialized = serde_json::to_string(&redacted).unwrap();

    assert!(!redacted.contains_key("_headers"));
    assert!(!serialized.contains(secret));
    assert_eq!(redacted["result"]["copy"], REDACTED);
}

#[test]
fn owned_and_borrowed_redaction_are_equivalent() {
    let secret = "equivalent-secret-123";
    let overlay = Context::from([(
        "_headers".to_string(),
        serde_json::json!({"authorization": secret, "code": 12345678}),
    )]);
    let redactor = SecretRedactor::from_overlay(&overlay);
    let context = Context::from([
        ("_headers".to_string(), overlay["_headers"].clone()),
        (
            "result".to_string(),
            serde_json::json!({
                "nested": [secret, {"copy": format!("prefix-{secret}-suffix")}],
                "numeric_copy": 12345678,
                "equivalent-secret-123": true,
                "safe": false
            }),
        ),
    ]);

    let borrowed = redactor.redact_context(&context);
    let owned = redactor.redact_context_owned(context);

    assert_eq!(owned, borrowed);
}

#[test]
fn empty_owned_redactor_reuses_string_allocation() {
    let text = "x".repeat(4096);
    let pointer = text.as_ptr();
    let context = Context::from([("result".to_string(), Value::String(text))]);

    let redacted = SecretRedactor::default().redact_context_owned(context);

    assert_eq!(redacted["result"].as_str().unwrap().as_ptr(), pointer);
}

#[test]
fn public_redaction_uses_legacy_headers_to_scrub_the_whole_record() {
    let mut value = serde_json::json!({
        "ctx": {
            "_headers": {"authorization": "Bearer old-secret-token"},
            "auth_token": "old-secret-token"
        },
        "tasks": {"check": {"error": "failed with old-secret-token"}},
        "safe": "visible"
    });

    redact_legacy_webhook_record(&mut value);

    assert!(value["ctx"].get("_headers").is_none());
    assert_eq!(value["ctx"]["auth_token"], REDACTED);
    assert!(
        !value["tasks"]["check"]["error"]
            .as_str()
            .unwrap()
            .contains("old-secret-token")
    );
    assert_eq!(value["safe"], "visible");
}

#[test]
fn legacy_ordinary_short_headers_do_not_corrupt_run_fields() {
    let mut value = serde_json::json!({
        "id": "run-2026",
        "ctx": {
            "_headers": {"content-length": "2", "authorization": "Bearer old-secret-token"},
            "copy": "old-secret-token"
        }
    });

    redact_legacy_webhook_record(&mut value);

    assert_eq!(value["id"], "run-2026");
    assert_eq!(value["ctx"]["copy"], REDACTED);
}

#[test]
fn structured_and_numeric_secret_forms_are_redacted() {
    let overlay = Context::from([(
        "_headers".to_string(),
        serde_json::json!({
            "x-signature": "{\"token\":\"long-secret-123\",\"code\":12345678}"
        }),
    )]);
    let redactor = SecretRedactor::from_overlay(&overlay);
    let context = Context::from([
        (
            "result".to_string(),
            Value::String("long-secret-123".to_string()),
        ),
        ("numeric_result".to_string(), serde_json::json!(12345678)),
        ("long-secret-123".to_string(), Value::Bool(true)),
    ]);

    let redacted = redactor.redact_context(&context);

    assert_eq!(redacted["result"], REDACTED);
    assert_eq!(redacted["numeric_result"], REDACTED);
    assert!(!redacted.contains_key("long-secret-123"));
}

#[test]
fn public_redaction_leaves_non_webhook_records_untouched() {
    let mut value = serde_json::json!({"ctx": {"signature": "business-value"}});

    redact_legacy_webhook_record(&mut value);

    assert_eq!(value["ctx"]["signature"], "business-value");
}

#[test]
fn public_redaction_handles_run_arrays() {
    let mut value = serde_json::json!([
        {"ctx": {"_headers": {"x-signature": "first-secret"}, "copy": "first-secret"}},
        {"ctx": {"safe": "visible"}}
    ]);

    redact_legacy_webhook_record(&mut value);

    assert!(value[0]["ctx"].get("_headers").is_none());
    assert_eq!(value[0]["ctx"]["copy"], REDACTED);
    assert_eq!(value[1]["ctx"]["safe"], "visible");
}

fn header_redactor(headers: Value) -> SecretRedactor {
    SecretRedactor::from_overlay(&Context::from([("_headers".to_string(), headers)]))
}

#[test]
fn short_components_and_header_names_are_not_secrets() {
    // IF-176: `true`, `false` and `1` are components of these values, and the
    // header names are not values at all.
    let redactor = header_redactor(serde_json::json!({
        "x-feature-flags": "beta=true,legacy=false",
        "x-amzn-trace-id": "Root=1-67891233-abcdef012345678912345678;Parent=53995c3f42cd8ad8;Sampled=1"
    }));
    let unrelated = Context::from([
        ("order1".to_string(), serde_json::json!("first")),
        ("total".to_string(), serde_json::json!(1)),
        ("items".to_string(), serde_json::json!([1, 2, 3])),
        ("note".to_string(), serde_json::json!("room 101, 1 item")),
        ("flag".to_string(), serde_json::json!("true")),
        ("x-feature-flags".to_string(), serde_json::json!("named")),
        (
            "mention".to_string(),
            serde_json::json!("read x-amzn-trace-id, got false"),
        ),
        (
            "nested".to_string(),
            serde_json::json!({"legacy1": {"beta": "false"}}),
        ),
    ]);

    assert_eq!(redactor.redact_context(&unrelated), unrelated);
    assert_eq!(redactor.redact_context_owned(unrelated.clone()), unrelated);
    assert_eq!(redactor.redact_live_context(unrelated.clone()), unrelated);
    assert_eq!(redactor.redact_text("true is 1"), "true is 1");
}

#[test]
fn whole_values_and_long_components_are_still_redacted() {
    let trace = "Root=1-67891233-abcdef012345678912345678;Parent=53995c3f42cd8ad8;Sampled=1";
    let redactor = header_redactor(serde_json::json!({
        "x-feature-flags": "beta=true,legacy=false",
        "x-amzn-trace-id": trace
    }));
    let context = Context::from([
        (
            "flags".to_string(),
            serde_json::json!("beta=true,legacy=false"),
        ),
        (
            "trace".to_string(),
            serde_json::json!(format!("trace {trace}")),
        ),
        ("parent".to_string(), serde_json::json!("53995c3f42cd8ad8")),
        (
            "root".to_string(),
            serde_json::json!("id 1-67891233-abcdef012345678912345678 seen"),
        ),
        ("53995c3f42cd8ad8".to_string(), serde_json::json!(true)),
    ]);

    let redacted = redactor.redact_context(&context);

    assert_eq!(redacted["flags"], REDACTED);
    assert_eq!(redacted["trace"], format!("trace {REDACTED}"));
    assert_eq!(redacted["parent"], REDACTED);
    assert_eq!(redacted["root"], format!("id {REDACTED} seen"));
    assert!(!redacted.contains_key("53995c3f42cd8ad8"));
    assert_eq!(redactor.redact_live_context(context.clone()), redacted);
    assert_eq!(redactor.redact_context_owned(context), redacted);
}

#[test]
fn structured_header_keys_and_short_scalars_are_not_secrets() {
    let redactor = header_redactor(serde_json::json!({
        "x-signature": "{\"token\":\"long-secret-123\",\"code\":12345678,\"kind\":\"v1\",\"n\":7}"
    }));
    let unrelated = Context::from([
        ("token".to_string(), serde_json::json!("kept")),
        ("kind".to_string(), serde_json::json!("v1")),
        ("code".to_string(), serde_json::json!(7)),
        (
            "text".to_string(),
            serde_json::json!("token code kind n v1 7"),
        ),
    ]);

    assert_eq!(redactor.redact_context(&unrelated), unrelated);
    assert_eq!(
        redactor.redact_text("got long-secret-123 and 12345678"),
        format!("got {REDACTED} and {REDACTED}")
    );
}

#[test]
fn padding_and_separators_do_not_become_secrets() {
    // `split_once('=')` on base64 padding used to register `=` as a secret.
    let redactor = header_redactor(serde_json::json!({
        "x-signature": "c2lnbmF0dXJlLXZhbHVl=="
    }));

    assert_eq!(redactor.redact_text("a=b, c==d"), "a=b, c==d");
    assert_eq!(redactor.redact_text("c2lnbmF0dXJlLXZhbHVl=="), REDACTED);
}

#[test]
fn short_whole_values_match_exactly_and_never_as_substrings() {
    // Not reachable from a webhook, which rejects values below the floor.
    let redactor = SecretRedactor::from_overlay(&Context::from([(
        "_pin".to_string(),
        serde_json::json!("1234"),
    )]));
    let context = Context::from([
        ("copy".to_string(), serde_json::json!("1234")),
        ("padded".to_string(), serde_json::json!(" 1234 ")),
        ("order".to_string(), serde_json::json!("order 12345")),
        ("key1234".to_string(), serde_json::json!("kept")),
        ("count".to_string(), serde_json::json!(12345)),
    ]);

    let redacted = redactor.redact_context(&context);

    assert_eq!(redacted["copy"], REDACTED);
    assert_eq!(redacted["padded"], REDACTED);
    assert_eq!(redacted["order"], "order 12345");
    assert_eq!(redacted["key1234"], "kept");
    assert_eq!(redacted["count"], 12345);
    assert_eq!(
        redactor.redact_text("pin 1234 rejected"),
        "pin 1234 rejected"
    );
}

#[test]
fn live_boundary_keeps_json_types_and_routing_keys() {
    let redactor = header_redactor(serde_json::json!({
        "x-request-timestamp": "1731400000",
        "x-feature-flags": "mode=needs_review"
    }));
    let output = Context::from([
        ("_route_gate".to_string(), serde_json::json!("needs_review")),
        (
            "_condition_result_gate".to_string(),
            serde_json::json!(true),
        ),
        ("_route_needs_review".to_string(), serde_json::json!("true")),
        ("event_time".to_string(), serde_json::json!(1731400000)),
        (
            "nested".to_string(),
            serde_json::json!({"times": [1731400000, 5], "_route_inner": "needs_review"}),
        ),
        ("label".to_string(), serde_json::json!("needs_review")),
        ("_headers".to_string(), serde_json::json!({"copy": "x"})),
    ]);

    let live = redactor.redact_live_context(output.clone());

    assert_eq!(live["_route_gate"], "needs_review");
    assert_eq!(live["_condition_result_gate"], true);
    assert_eq!(live["_route_needs_review"], "true");
    assert_eq!(live["event_time"], 1731400000);
    assert_eq!(live["nested"]["times"], serde_json::json!([1731400000, 5]));
    // Only top-level routing keys are engine input; everything else is data.
    assert_eq!(live["nested"]["_route_inner"], REDACTED);
    assert_eq!(live["label"], REDACTED);
    assert!(!live.contains_key("_headers"));

    // The record boundary withholds what the live boundary had to keep.
    let record = redactor.redact_context(&live);
    assert_eq!(record["_route_gate"], REDACTED);
    assert_eq!(record["event_time"], REDACTED);
    assert_eq!(record["nested"]["times"], serde_json::json!([REDACTED, 5]));
    assert!(!record.contains_key("_route_needs_review"));
    assert_eq!(record, redactor.redact_context_owned(output));
}

#[test]
fn resolved_credentials_are_redacted_whatever_their_length() {
    // IF-108: the llm node resolves these itself, so the IF-176 floor for
    // text a remote party chose does not apply.
    let redactor = SecretRedactor::from_credentials(["Bearer k", "llm", "", "  "]);

    let rejected = redactor.redact_text("Incorrect API key provided: k; rejected k");
    assert!(!rejected.contains('k'), "{rejected}");
    assert!(rejected.contains(REDACTED));
    assert_eq!(
        redactor.redact_text("header Bearer k, model llm"),
        format!("header {REDACTED}, model {REDACTED}")
    );
    // An empty credential must not turn every position into a match.
    assert_eq!(redactor.redact_text("status 401"), "status 401");
}

#[test]
fn resolved_credentials_match_their_json_escaped_form() {
    let key = "q\"s\\v";
    let redactor = SecretRedactor::from_credentials([key]);
    let echoed = serde_json::json!({ "message": format!("rejected {key}") }).to_string();

    let redacted = redactor.redact_text(&echoed);

    assert!(!redacted.contains("q\\\"s"), "{redacted}");
    assert!(redacted.contains(REDACTED));
}

#[test]
fn overlay_values_keep_the_floor_that_resolved_credentials_do_not() {
    // The same short header value must not rewrite ordinary text when it
    // arrives as an overlay value (IF-176).
    let overlay = header_redactor(serde_json::json!({ "authorization": "Bearer k" }));
    let text = "Incorrect API key provided: k; rejected k";

    assert_eq!(overlay.redact_text(text), text);
    assert_eq!(overlay.redact_text("Bearer k"), REDACTED);
}
