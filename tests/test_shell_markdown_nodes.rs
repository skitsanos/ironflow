//! Tests for shell_command and html_to_markdown nodes.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::{NodeFailure, NodeRegistry};

// --- Helpers ---

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

// =============================================================================
// shell_command tests
// =============================================================================

#[tokio::test]
async fn shell_echo_hello() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "echo",
        "args": ["hello"]
    });

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
    let stdout = result.get("shell_stdout").unwrap().as_str().unwrap();
    assert!(
        stdout.contains("hello"),
        "Expected stdout to contain 'hello', got: {stdout}"
    );
}

#[tokio::test]
async fn shell_echo_with_multiple_args() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "echo",
        "args": ["hello", "world"]
    });

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
    let stdout = result.get("shell_stdout").unwrap().as_str().unwrap();
    assert!(
        stdout.contains("hello world"),
        "Expected stdout to contain 'hello world', got: {stdout}"
    );
}

#[tokio::test]
async fn shell_failing_command_returns_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "false"
    });

    let result = node.execute(&config, &empty_ctx()).await;
    assert!(
        result.is_err(),
        "Expected failing command to return an error"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exited with code"),
        "Expected error about exit code, got: {err_msg}"
    );
}

#[tokio::test]
async fn shell_with_env_vars() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "sh",
        "args": ["-c", "echo $MY_TEST_VAR"],
        "env": {
            "MY_TEST_VAR": "ironflow_test_value"
        }
    });

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
    let stdout = result.get("shell_stdout").unwrap().as_str().unwrap();
    assert!(
        stdout.contains("ironflow_test_value"),
        "Expected stdout to contain env var value, got: {stdout}"
    );
}

#[tokio::test]
async fn shell_interpolates_nested_and_indexed_config_values() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();
    let working_directory = tempfile::tempdir().unwrap();
    std::fs::write(working_directory.path().join("marker.txt"), "present").unwrap();

    let ctx = ctx_with(vec![(
        "shell",
        serde_json::json!({
            "command": "sh",
            "arguments": ["-c"],
            "directories": [working_directory.path().to_string_lossy()],
            "environment": { "message": "nested-value" },
            "labels": ["indexed-value"]
        }),
    )]);
    let config = serde_json::json!({
        "cmd": "${ctx.shell.command}",
        "args": [
            "${ctx.shell.arguments[0]}",
            "test -f marker.txt && printf '%s|%s|%s' \"$IF_MESSAGE\" \"${TMPDIR:-/tmp}\" \"${ctx.shell.labels[0]}\""
        ],
        "cwd": "${ctx.shell.directories[0]}",
        "env": {
            "IF_MESSAGE": "${ctx.shell.environment.message}",
            "TMPDIR": "/tmp/foreign-template"
        }
    });

    let result = node.execute(&config, &ctx).await.unwrap();
    assert_eq!(
        result
            .get("shell_stdout")
            .and_then(serde_json::Value::as_str),
        Some("nested-value|/tmp/foreign-template|indexed-value")
    );
}

#[tokio::test]
async fn shell_rejects_non_string_arguments() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();
    let config = serde_json::json!({
        "cmd": "echo",
        "args": ["valid", 42]
    });

    let error = node.execute(&config, &empty_ctx()).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("expects every 'args' entry to be a string")
    );
}

#[tokio::test]
async fn shell_rejects_non_string_environment_values() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();
    let config = serde_json::json!({
        "cmd": "echo",
        "env": { "INVALID": true }
    });

    let error = node.execute(&config, &empty_ctx()).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("expects every 'env' value to be a string")
    );
}

#[tokio::test]
async fn shell_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "echo",
        "args": ["test"],
        "output_key": "myout"
    });

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
    assert!(result.contains_key("myout_stdout"));
    assert!(result.contains_key("myout_code"));
    assert!(result.contains_key("myout_success"));
}

// =============================================================================
// html_to_markdown tests
// =============================================================================

#[tokio::test]
async fn html_to_markdown_basic_tags() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "input": "<h1>Title</h1><p>A <b>bold</b> paragraph.</p>"
    });

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
    let md = result.get("markdown").unwrap().as_str().unwrap();
    assert!(
        md.contains("Title"),
        "Expected markdown to contain 'Title', got: {md}"
    );
    assert!(
        md.contains("**bold**"),
        "Expected markdown to contain '**bold**', got: {md}"
    );
}

#[tokio::test]
async fn html_to_markdown_with_links() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "input": "<p>Visit <a href=\"https://example.com\">Example</a> site.</p>"
    });

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
    let md = result.get("markdown").unwrap().as_str().unwrap();
    assert!(
        md.contains("[Example](https://example.com)"),
        "Expected markdown link, got: {md}"
    );
}

#[tokio::test]
async fn html_to_markdown_empty_input() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "input": ""
    });

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
    let md = result.get("markdown").unwrap().as_str().unwrap();
    assert!(md.trim().is_empty(), "Expected empty markdown, got: {md}");
}

#[tokio::test]
async fn html_to_markdown_via_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "source_key": "my_html"
    });

    let ctx = ctx_with(vec![("my_html", serde_json::json!("<h2>Heading</h2>"))]);

    let result = node.execute(&config, &ctx).await.unwrap();
    let md = result.get("markdown").unwrap().as_str().unwrap();
    assert!(
        md.contains("Heading"),
        "Expected markdown to contain 'Heading', got: {md}"
    );
}

#[tokio::test]
async fn html_to_markdown_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "input": "<p>text</p>",
        "output_key": "md_out"
    });

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
    assert!(
        result.contains_key("md_out"),
        "Expected custom output key 'md_out'"
    );
}

async fn html_to_markdown(input: &str) -> String {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();
    let result = node
        .execute(&serde_json::json!({ "input": input }), &empty_ctx())
        .await
        .unwrap();
    result
        .get("markdown")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

fn render(markdown: &str) -> String {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    comrak::markdown_to_html(markdown, &options)
}

#[tokio::test]
async fn html_to_markdown_keeps_lists_tables_code_images_and_quotes_renderable() {
    let md = html_to_markdown(concat!(
        "<h2>Section</h2>",
        "<ul><li>one</li><li>two<ul><li>nested</li></ul></li></ul>",
        "<ol><li>first</li><li>second</li></ol>",
        "<table><thead><tr><th>Name</th><th>Count</th></tr></thead>",
        "<tbody><tr><td>Sample</td><td>48</td></tr></tbody></table>",
        "<p>Inline <code>x*y</code> and <img alt=\"diagram\" src=\"diagram.png\"></p>",
        "<pre><code>line one\n*raw*</code></pre>",
        "<blockquote><p>quoted</p></blockquote><hr>",
        "<p>Visit <a href=\"https://example.com/a?b=1&amp;c=2\">Example</a>.</p>",
    ))
    .await;
    let rendered = render(&md);
    for semantic in [
        "<h2>Section</h2>",
        "<li>one</li>",
        "<li>nested</li>",
        "<ol>",
        "<li>first</li>",
        "<table>",
        "<th>Name</th>",
        "<td>Sample</td>",
        "<td>48</td>",
        "<code>x*y</code>",
        "<img src=\"diagram.png\" alt=\"diagram\"",
        "<pre><code>line one\n*raw*\n</code></pre>",
        "<blockquote>",
        "<hr />",
        "<a href=\"https://example.com/a?b=1&amp;c=2\">Example</a>",
    ] {
        assert!(
            rendered.contains(semantic),
            "lost {semantic}\n{md}\n{rendered}"
        );
    }
}

#[tokio::test]
async fn html_to_markdown_escapes_literal_markdown_punctuation() {
    let md = html_to_markdown(concat!(
        "<p># not a heading</p>",
        "<p>*stars* and _unders_ and C# issue #7</p>",
        "<p><code>*code*\\path</code></p>",
    ))
    .await;
    assert!(md.contains(r"\# not a heading"), "{md}");
    assert!(md.contains(r"\*stars\*"), "{md}");
    assert!(md.contains("C# issue #7"), "{md}");
    assert!(md.contains(r"`*code*\path`"), "{md}");
    let rendered = render(&md);
    assert!(rendered.contains("<p># not a heading</p>"), "{rendered}");
    assert!(
        rendered.contains("*stars* and _unders_ and C# issue #7"),
        "{rendered}"
    );
    assert!(rendered.contains(r"<code>*code*\path</code>"), "{rendered}");
}

#[tokio::test]
async fn html_to_markdown_omits_head_script_style_and_fallback_content() {
    let md = html_to_markdown(concat!(
        "<!doctype html><html><head><title>Page Title</title>",
        "<style>body{color:red}</style><script>secret()</script></head>",
        "<body><p>Visible</p><script>bodySecret()</script>",
        "<noscript>fallback</noscript><template>inert</template></body></html>",
    ))
    .await;
    assert!(md.contains("Visible"), "{md}");
    for hidden in ["Page Title", "color:red", "secret()", "fallback", "inert"] {
        assert!(!md.contains(hidden), "leaked {hidden}: {md}");
    }
}

// --- Shell output bounded by IRONFLOW_MAX_SHELL_OUTPUT_BYTES ---

#[tokio::test]
async fn shell_stdout_truncated_at_cap() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    // Cap very low, then produce far more output — the node should
    // truncate and mark the result rather than holding the full payload.
    unsafe {
        std::env::set_var("IRONFLOW_MAX_SHELL_OUTPUT_BYTES", "128");
    }

    // `yes` would loop forever; use a bounded producer for determinism.
    // 2 KiB of `x\n` = 1024 lines.
    let config = serde_json::json!({
        "cmd": "sh",
        "args": ["-c", "printf 'x%.0s' $(seq 1 2048)"]
    });

    let result = node.execute(&config, &empty_ctx()).await;
    let failure = node
        .execute(
            &serde_json::json!({
                "cmd": "sh",
                "args": [
                    "-c",
                    "printf 'o%.0s' $(seq 1 2048); printf 'e%.0s' $(seq 1 2048) >&2; exit 6"
                ]
            }),
            &empty_ctx(),
        )
        .await;

    unsafe {
        std::env::remove_var("IRONFLOW_MAX_SHELL_OUTPUT_BYTES");
    }

    let out = result.expect("shell_command should succeed even when output is truncated");
    let stdout = out.get("shell_stdout").unwrap().as_str().unwrap();
    assert!(
        stdout.len() <= 128,
        "stdout must not exceed the configured cap; got {} bytes",
        stdout.len()
    );
    assert_eq!(
        out.get("shell_output_truncated"),
        Some(&serde_json::json!(true)),
        "truncation marker must be set when output is capped"
    );

    let failure = failure.unwrap_err();
    let output = failure.downcast_ref::<NodeFailure>().unwrap().output();
    assert!(output["shell_stdout"].as_str().unwrap().len() <= 128);
    assert!(output["shell_stderr"].as_str().unwrap().len() <= 128);
    assert_eq!(output["shell_code"], 6);
    assert_eq!(output["shell_success"], false);
    assert_eq!(output["shell_output_truncated"], true);
}
