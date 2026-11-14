//! IF-197: the `llm` node rejects unknown providers before any request, sends
//! the documented parameters, and fails a step whose provider reply is empty.

#[path = "provider_http/support.rs"]
mod support;

use axum::http::StatusCode;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};
use support::{Server, success};

const KEY: &str = "synthetic-contract-key";

async fn run(config: Value) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    run_with(config, &Context::new()).await
}

async fn run_with(
    config: Value,
    ctx: &Context,
) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    NodeRegistry::with_builtins()
        .get("llm")
        .unwrap()
        .execute(&config, ctx)
        .await
}

fn body(server: &Server) -> Value {
    let requests = server.requests();
    assert_eq!(requests.len(), 1, "expected exactly one provider request");
    assert_eq!(requests[0].method, axum::http::Method::POST);
    serde_json::from_slice(&requests[0].body).unwrap()
}

fn custom(server: &Server, mode: &str) -> Value {
    json!({
        "provider": "custom", "base_url": server.base, "auth_type": "none",
        "mode": mode, "prompt": "ordinary input"
    })
}

fn with(mut config: Value, extra: Value) -> Value {
    config
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    config
}

#[tokio::test]
async fn unknown_provider_is_rejected_before_any_request() {
    for provider in [
        json!("openai-compatible"),
        json!("ollama"),
        json!("gemini"),
        json!(""),
        json!(7),
        json!(true),
    ] {
        let server = Server::start(None, success()).await;
        let error = run(json!({
            "provider": provider, "base_url": server.base, "api_key": KEY,
            "prompt": "ordinary input"
        }))
        .await
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("unsupported provider") || message.contains("must be a string"),
            "{message}"
        );
        for accepted in ["'openai'", "'openai_compatible'", "'azure'", "'custom'"] {
            assert!(message.contains(accepted), "{message}");
        }
        assert!(!message.contains(KEY), "credential reached the error");
        assert!(
            server.requests().is_empty(),
            "provider {provider} still sent a request"
        );
    }
}

/// The scenario behind the priority: the operator configured a gateway for
/// the compatible provider, and a typo must not send the prompt and the
/// OpenAI key to the OpenAI base URL instead.
#[tokio::test]
async fn mistyped_provider_does_not_fall_back_to_the_openai_configuration() {
    for provider in ["openai-compatible", "ollama"] {
        let openai = Server::start(None, success()).await;
        let gateway = Server::start(None, success()).await;
        let directory = tempfile::tempdir().unwrap();
        let flow_path = directory.path().join("flow.lua");
        std::fs::write(
            &flow_path,
            format!(
                r#"
            local flow = Flow.new("mistyped-provider")
            flow:step("request", nodes.llm({{
                provider = "{provider}", prompt = "private prompt text"
            }}))
            return flow
        "#
            ),
        )
        .unwrap();
        let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .current_dir(directory.path())
            .env_clear()
            .env("OPENAI_API_KEY", KEY)
            .env("OPENAI_BASE_URL", &openai.base)
            .env("OPENAI_COMPATIBLE_BASE_URL", &gateway.base)
            .arg("run")
            .arg(&flow_path)
            .arg("--store-dir")
            .arg(directory.path().join("runs"))
            .kill_on_drop(true)
            .output()
            .await
            .unwrap();
        let console = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let leaked: Vec<_> = openai
            .requests()
            .iter()
            .map(|request| {
                let sent = String::from_utf8_lossy(&request.body).into_owned();
                let authorized = request
                    .headers
                    .get("authorization")
                    .is_some_and(|value| value.as_bytes().ends_with(KEY.as_bytes()));
                format!(
                    "{} prompt_sent={} key_sent={authorized}",
                    request.path,
                    sent.contains("private prompt text")
                )
            })
            .collect();
        assert!(
            leaked.is_empty(),
            "'{provider}' reached the OpenAI base URL: {leaked:?}"
        );
        assert!(gateway.requests().is_empty());
        assert_ne!(output.status.code(), Some(0), "{console}");
        assert!(console.contains("unsupported provider"), "{console}");
        assert!(!console.contains(KEY), "credential reached CLI output");
    }
}

#[tokio::test]
async fn supported_providers_keep_their_routes() {
    let cases = [
        (json!({"provider": "openai"}), "/chat/completions"),
        (json!({}), "/chat/completions"),
        (json!({"provider": "OpenAI"}), "/chat/completions"),
        (
            json!({"provider": "openai_compatible"}),
            "/chat/completions",
        ),
        (json!({"provider": "compatible"}), "/chat/completions"),
        (
            json!({"provider": "custom", "chat_path": "/v9/chat"}),
            "/v9/chat",
        ),
        (
            json!({"provider": "azure", "azure_chat_deployment": "deployment"}),
            "/openai/deployments/deployment/chat/completions",
        ),
    ];
    for (provider, path) in cases {
        let server = Server::start(None, success()).await;
        let output = run(with(
            json!({
                "base_url": server.base, "azure_endpoint": server.base,
                "api_key": KEY, "prompt": "ordinary input"
            }),
            provider.clone(),
        ))
        .await
        .unwrap();
        assert_eq!(output["llm_text"], "accepted", "{provider}");
        let requests = server.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, path, "{provider}");
    }
}

#[tokio::test]
async fn top_level_response_format_reaches_the_chat_request() {
    let format = json!({"type": "json_object"});
    let server = Server::start(None, success()).await;
    run(with(
        custom(&server, "chat"),
        json!({"response_format": format}),
    ))
    .await
    .unwrap();
    assert_eq!(body(&server)["response_format"], format);

    // `extra` is merged last and keeps working; it wins when both are given.
    let server = Server::start(None, success()).await;
    let schema = json!({"type": "json_schema", "json_schema": {"name": "fixture"}});
    run(with(
        custom(&server, "chat"),
        json!({"response_format": format, "extra": {"response_format": schema}}),
    ))
    .await
    .unwrap();
    assert_eq!(body(&server)["response_format"], schema);

    let server = Server::start(None, success()).await;
    let error = run(with(
        custom(&server, "chat"),
        json!({"response_format": "json_object"}),
    ))
    .await
    .unwrap_err();
    assert!(format!("{error:#}").contains("'response_format' must be an object"));
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn input_key_has_no_implicit_default() {
    let server = Server::start(None, success()).await;
    let mut config = custom(&server, "chat");
    config.as_object_mut().unwrap().remove("prompt");
    let ctx = Context::from([("prompt".to_string(), json!("context text never named"))]);
    let error = run_with(config.clone(), &ctx).await.unwrap_err();
    assert!(format!("{error:#}").contains("'input_key'"));
    assert!(server.requests().is_empty());

    let output = run_with(with(config, json!({"input_key": "prompt"})), &ctx)
        .await
        .unwrap();
    assert_eq!(output["llm_text"], "accepted");
    assert_eq!(
        body(&server)["messages"][0]["content"],
        "context text never named"
    );
}

#[tokio::test]
async fn successful_response_without_a_usable_reply_fails_the_step() {
    let cases = [
        ("chat", json!({"choices": []})),
        ("chat", json!({"id": "fixture"})),
        ("chat", json!("not a completion document")),
        (
            "chat",
            json!({"choices": [{"message": {"content": null}, "finish_reason": "length"}]}),
        ),
        (
            "chat",
            json!({"choices": [{"message": {"content": "", "tool_calls": []}}]}),
        ),
        (
            "chat",
            json!({"choices": [{"message": {"content": null, "refusal": "declined"}}]}),
        ),
        ("responses", json!({"output": []})),
        ("responses", json!({"output_text": ""})),
        (
            "responses",
            json!({"status": "incomplete", "output": [
                {"type": "reasoning", "id": "rs_fixture", "summary": []}
            ]}),
        ),
    ];
    for (mode, reply) in cases {
        let server = Server::start(None, (StatusCode::OK, reply.clone())).await;
        let result = run(custom(&server, mode)).await;
        let error = match result {
            Ok(output) => panic!(
                "{mode} {reply} reported success with text {}",
                output["llm_text"]
            ),
            Err(error) => format!("{error:#}"),
        };
        assert!(error.contains("no usable reply"), "{error}");
        assert!(error.contains("200"), "{error}");
        assert!(!error.contains("<no reply>"), "{error}");
        assert!(!error.contains("rs_fixture") && !error.contains("declined"));
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn empty_reply_error_names_the_finish_reason() {
    let reply = json!({"choices": [{"message": {"content": ""}, "finish_reason": "length"}]});
    let server = Server::start(None, (StatusCode::OK, reply)).await;
    let error = run(custom(&server, "chat")).await.unwrap_err();
    assert!(format!("{error:#}").contains("finish_reason 'length'"));
}

#[tokio::test]
async fn tool_call_only_replies_remain_successful() {
    let chat = json!({"choices": [{"message": {"content": null, "tool_calls": [{
        "id": "call_fixture", "type": "function",
        "function": {"name": "lookup", "arguments": "{\"q\":1}"}
    }]}}]});
    let server = Server::start(None, (StatusCode::OK, chat)).await;
    let output = run(custom(&server, "chat")).await.unwrap();
    assert_eq!(output["llm_tool_call_needed"], true);
    assert_eq!(output["llm_text"], "<tool call requested>");
    assert_eq!(output["llm_tool_call_names"], json!(["lookup"]));

    let responses = json!({"output": [
        {"type": "reasoning", "id": "rs_fixture", "summary": []},
        {"type": "function_call", "id": "fc_fixture", "call_id": "call_fixture",
         "name": "lookup", "arguments": "{\"q\":1}"}
    ]});
    let server = Server::start(None, (StatusCode::OK, responses)).await;
    let output = run(custom(&server, "responses")).await.unwrap();
    assert_eq!(output["llm_tool_call_needed"], true);
    assert_eq!(output["llm_text"], "<tool call requested>");
    assert_eq!(output["llm_tool_call_names"], json!(["lookup"]));
    let normalized = &output["llm_tool_calls_normalized"][0];
    assert_eq!(normalized["id"], "call_fixture");
    assert_eq!(normalized["arguments"], json!({"q": 1}));
}

#[tokio::test]
async fn responses_text_is_read_from_message_items() {
    let reply = json!({"output": [
        {"type": "reasoning", "id": "rs_fixture", "summary": []},
        {"type": "message", "role": "assistant", "content": [
            {"type": "output_text", "text": "first"},
            {"type": "output_text", "text": "second"}
        ]}
    ]});
    let server = Server::start(None, (StatusCode::OK, reply)).await;
    let output = run(custom(&server, "responses")).await.unwrap();
    assert_eq!(output["llm_text"], "first second");
    assert_eq!(output["llm_tool_call_needed"], false);
}

#[tokio::test]
async fn responses_mode_maps_the_system_prompt_and_rejects_chat_only_parameters() {
    for key in ["system_prompt", "system"] {
        let server = Server::start(None, success()).await;
        run(with(
            custom(&server, "responses"),
            json!({key: "be brief", "max_tokens": 9}),
        ))
        .await
        .unwrap();
        let sent = body(&server);
        assert_eq!(sent["instructions"], "be brief");
        assert_eq!(sent["input"], "ordinary input");
        assert_eq!(sent["max_output_tokens"], 9);
        assert!(sent.get("messages").is_none());
    }

    let tool = json!([{"type": "function", "function": {"name": "lookup"}}]);
    for (key, value) in [
        ("tools", tool.clone()),
        ("tool_choice", json!("required")),
        ("response_format", json!({"type": "json_object"})),
    ] {
        let server = Server::start(None, success()).await;
        let error = run(with(custom(&server, "responses"), json!({key: value})))
            .await
            .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(&format!("'{key}'")), "{message}");
        assert!(message.contains("responses mode"), "{message}");
        assert!(message.contains("extra"), "{message}");
        assert!(server.requests().is_empty(), "{key} still sent a request");
    }

    // Responses-format fields keep passing through `extra` unchanged.
    let server = Server::start(None, success()).await;
    let extra = json!({
        "tools": [{"type": "web_search_preview"}], "tool_choice": "auto",
        "instructions": "from extra"
    });
    run(with(
        custom(&server, "responses"),
        json!({"system_prompt": "from parameter", "extra": extra}),
    ))
    .await
    .unwrap();
    let sent = body(&server);
    assert_eq!(sent["tools"], extra["tools"]);
    assert_eq!(sent["tool_choice"], "auto");
    assert_eq!(sent["instructions"], "from extra");
}

#[tokio::test]
async fn chat_mode_still_sends_tools_and_the_system_message() {
    let tool = json!([{"type": "function", "function": {"name": "lookup"}}]);
    let server = Server::start(None, success()).await;
    run(with(
        custom(&server, "chat"),
        json!({"system_prompt": "be brief", "tools": tool, "tool_choice": "required"}),
    ))
    .await
    .unwrap();
    let sent = body(&server);
    assert_eq!(sent["tools"], tool);
    assert_eq!(sent["tool_choice"], "required");
    assert_eq!(
        sent["messages"],
        json!([
            {"role": "system", "content": "be brief"},
            {"role": "user", "content": "ordinary input"}
        ])
    );
    assert!(sent.get("instructions").is_none());
}

#[tokio::test]
async fn embedding_nodes_reject_unknown_and_non_string_providers_before_any_request() {
    let registry = NodeRegistry::with_builtins();
    let ctx = Context::from([
        ("texts".to_string(), json!(["first", "second"])),
        (
            "text".to_string(),
            json!("First sentence. Second sentence."),
        ),
    ]);
    for node in ["ai_embed", "ai_chunk_semantic"] {
        for (provider, expected) in [
            (json!("open-ai"), "unsupported provider 'open-ai'"),
            (json!("OpenAI"), "Use 'openai', 'ollama', or 'oauth'."),
            (json!(7), "'provider' must be a string"),
        ] {
            let server = Server::start(None, success()).await;
            let error = registry
                .get(node)
                .unwrap()
                .execute(
                    &json!({
                        "provider": provider, "base_url": server.base, "api_key": KEY,
                        "input_key": "texts", "source_key": "text"
                    }),
                    &ctx,
                )
                .await
                .unwrap_err();
            let message = format!("{error:#}");
            assert!(message.contains(expected), "{node}: {message}");
            assert!(!message.contains(KEY));
            assert!(server.requests().is_empty(), "{node} sent a request");
        }
    }
}
