use std::collections::HashMap;

use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[tokio::test]
async fn markdown_autolinks_handle_many_email_addresses_without_stack_overflow() {
    let node = NodeRegistry::with_builtins()
        .get("markdown_to_html")
        .unwrap();
    // GHSA-xg9p-p4jc-c46g: the previous parser recursed once per email.
    let count = 30_000;
    let output = node
        .execute(&json!({"input": "a@b.co ".repeat(count)}), &HashMap::new())
        .await
        .unwrap();
    let html = output["html"].as_str().unwrap();
    assert_eq!(html.matches("href=\"mailto:a@b.co\"").count(), count);
}

#[tokio::test]
async fn markdown_autolinks_preserve_long_unmatched_parenthesis_suffixes() {
    let node = NodeRegistry::with_builtins()
        .get("markdown_to_html")
        .unwrap();
    let count = 65_536;
    let output = node
        .execute(
            &json!({"input": format!("https://example.com{}", ")".repeat(count))}),
            &HashMap::new(),
        )
        .await
        .unwrap();
    let html = output["html"].as_str().unwrap();
    assert!(html.contains("href=\"https://example.com\""));
    assert_eq!(html.matches(')').count(), count);
}
