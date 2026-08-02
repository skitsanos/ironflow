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
