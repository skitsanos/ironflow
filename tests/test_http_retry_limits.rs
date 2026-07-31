//! Bounds and strict parsing for the HTTP node's response-status retry loop.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

async fn execute(config: serde_json::Value) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    execute_with_ctx(config, Context::new()).await
}

async fn execute_with_ctx(
    config: serde_json::Value,
    ctx: Context,
) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    NodeRegistry::with_builtins()
        .get("http_request")
        .expect("http_request must be registered")
        .execute(&config, &ctx)
        .await
}

#[tokio::test]
async fn status_retry_count_is_strict_and_bounded_before_network_io() {
    for (value, expected) in [
        (serde_json::json!(101), "max 100"),
        (serde_json::json!("many"), "non-negative whole number"),
    ] {
        let error = execute(serde_json::json!({
            "url": "http://127.0.0.1:9",
            "status_retries": value,
        }))
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains(expected), "unexpected error: {error}");
    }
}

#[tokio::test]
async fn enabled_status_retries_require_positive_delay_ceilings() {
    for (key, value) in [
        ("status_retry_backoff", serde_json::json!(0)),
        ("max_retry_after", serde_json::json!(0)),
    ] {
        let mut config = serde_json::json!({
            "url": "http://127.0.0.1:9",
            "status_retries": 1,
        });
        config[key] = value;

        let error = execute(config).await.unwrap_err().to_string();
        assert!(
            error.contains("must be at least 0.01 seconds"),
            "unexpected error for {key}: {error}"
        );
    }
}

#[tokio::test]
async fn invalid_ssrf_toggle_fails_closed_before_network_io() {
    let error = execute(serde_json::json!({
        "url": "http://127.0.0.1:9",
        "block_private_network": "treu",
    }))
    .await
    .unwrap_err()
    .to_string();

    assert!(
        error.contains("'block_private_network' must be a boolean"),
        "{error}"
    );
}

#[tokio::test]
async fn redirect_count_is_strict_and_bounded_before_network_io() {
    for (value, expected) in [
        (serde_json::json!(101), "max 100"),
        (serde_json::json!("many"), "non-negative whole number"),
    ] {
        let error = execute(serde_json::json!({
            "url": "http://127.0.0.1:9",
            "max_redirects": value,
        }))
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains(expected), "unexpected error: {error}");
    }
}

#[tokio::test]
async fn retry_statuses_reject_values_outside_the_http_status_code_range() {
    for value in [serde_json::json!(99), serde_json::json!(1000)] {
        let error = execute(serde_json::json!({
            "url": "http://127.0.0.1:9",
            "retry_statuses": [value],
        }))
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("valid HTTP status codes"), "{error}");
    }

    let ctx = Context::from([("retry_status".to_string(), serde_json::json!(65535))]);
    let error = execute_with_ctx(
        serde_json::json!({
            "url": "http://127.0.0.1:9",
            "retry_statuses": ["${ctx.retry_status}"],
        }),
        ctx,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("valid HTTP status codes"), "{error}");
}
