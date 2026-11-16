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

#[tokio::test]
async fn html_to_markdown_charges_its_output_against_the_extract_output_limit() {
    // IF-215: the Markdown is 13 bytes.
    let html = "<p>sixteen bytes</p>";
    let _limit = EnvOverride::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "13");
    assert_eq!(html_to_markdown(html).await.unwrap(), "sixteen bytes");

    let _limit = EnvOverride::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "12");
    let error = html_to_markdown(html).await.unwrap_err().to_string();
    assert_eq!(
        error,
        "html_to_markdown: Markdown output exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES (12)"
    );
}

#[tokio::test]
async fn html_to_markdown_charges_repeated_prefixes_before_the_output_exists() {
    // Four lines under three quote levels: 16 bytes of text and line breaks,
    // 24 bytes of prefixes. The prefixes alone pass a 20-byte limit.
    let html = "<blockquote><blockquote><blockquote><pre>a\nb\nc\nd\n</pre></blockquote></blockquote></blockquote>";
    let _limit = EnvOverride::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "64");
    assert_eq!(
        html_to_markdown(html).await.unwrap(),
        "> > > a\n> > > b\n> > > c\n> > > d"
    );

    let _limit = EnvOverride::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "20");
    let error = html_to_markdown(html).await.unwrap_err().to_string();
    assert_eq!(
        error,
        "html_to_markdown: Markdown output exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES (20)"
    );
}
