//! IF-174: `html_to_markdown` admits its input under the same byte and
//! markup-item limits as `extract_html` before any HTML is parsed.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

struct EnvOverride {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvOverride {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        // The serial test binary runs one test at a time.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvOverride {
    fn drop(&mut self) {
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

async fn html_to_markdown(html: &str) -> anyhow::Result<String> {
    let context = Context::from([("page".to_string(), json!(html))]);
    let output = NodeRegistry::with_builtins()
        .get("html_to_markdown")
        .unwrap()
        .execute(&json!({ "source_key": "page" }), &context)
        .await?;
    Ok(output["markdown"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn html_to_markdown_charges_markup_items_before_parsing() {
    let html = "<p>one</p><p>two</p><p>three</p>";
    let _limit = EnvOverride::set("IRONFLOW_MAX_EXTRACT_ITEMS", "6");
    assert_eq!(html_to_markdown(html).await.unwrap(), "one\n\ntwo\n\nthree");

    let _limit = EnvOverride::set("IRONFLOW_MAX_EXTRACT_ITEMS", "5");
    let error = html_to_markdown(html).await.unwrap_err().to_string();
    assert!(
        error.starts_with("html_to_markdown: ") && error.contains("IRONFLOW_MAX_EXTRACT_ITEMS (5)"),
        "{error}"
    );
}

#[tokio::test]
async fn html_to_markdown_rejects_input_above_the_file_byte_limit() {
    let html = "<p>sixteen bytes</p>";
    assert_eq!(html.len(), 20);
    let _limit = EnvOverride::set("IRONFLOW_MAX_FILE_BYTES", "20");
    assert_eq!(html_to_markdown(html).await.unwrap(), "sixteen bytes");

    let _limit = EnvOverride::set("IRONFLOW_MAX_FILE_BYTES", "19");
    let error = html_to_markdown(html).await.unwrap_err().to_string();
    assert!(
        error.starts_with("html_to_markdown: ")
            && error.contains("20 bytes")
            && error.contains("IRONFLOW_MAX_FILE_BYTES (19)"),
        "{error}"
    );
}
