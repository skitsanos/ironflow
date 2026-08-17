# `llm`

Run a chat-style request against OpenAI, OpenAI-compatible, Azure, or custom endpoints using one node and one consistent output shape.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `provider` | string | no | `"openai"` | Provider backend: `"openai"`, `"openai_compatible"`, `"azure"`, `"custom"` |
| `mode` | string | no | `"chat"` | Request mode: `"chat"`, `"responses"`, or `"auto"` |
| `model` | string | no | provider-dependent | Model name (for Azure, defaults to deployment when available) |
| `tools` | array | no | — | OpenAI-style tool definitions, provided as a Lua table |
| `tool_choice` | string/object | no | `"auto"` | Tool selection behavior (`"auto"`, `"required"`, or explicit object) |
| `prompt` | string | no | — | Direct prompt text for user content |
| `input_key` | string | no | `"prompt"` | Context key for prompt text when `prompt` is not set |
| `messages` | array | no | — | Chat-style message objects (`role`, `content`) for chat mode |
| `messages_key` | string | no | — | Context key containing a runtime chat-message array. Mutually exclusive with `messages`. |
| `system_prompt` | string | no | — | System message used when building chat `messages` automatically |
| `system` | string | no | — | Alias for `system_prompt` |
| `temperature` | number | no | — | Sampling temperature |
| `max_tokens` | number | no | — | OpenAI chat `max_tokens` (mapped to `max_tokens` for chat and `max_output_tokens` for responses) |
| `max_output_tokens` | number | no | — | Output-token limit. Mapped to `max_completion_tokens` for chat mode and `max_output_tokens` for responses mode. |
| `response_format` | object | no | — | OpenAI-compatible response format override. Useful aliases: `{ type = "json_object" }` or `{ type = "json_schema", json_schema = { ... } }` |
| `extra` | object | no | — | Extra request fields merged into payload |
| `output_key` | string | no | `"llm"` | Prefix for output context keys |
| `timeout` | number | no | `30` | Request timeout in seconds |
| `max_response_bytes` | number/string | no | `IRONFLOW_LLM_MAX_RESPONSE_BYTES` / `26214400` | Maximum provider response body size before failing. |
| `max_image_input_bytes` | number/string | no | `IRONFLOW_LLM_MAX_IMAGE_INPUT_BYTES` / `52428800` | Maximum cumulative raw bytes across image-artifact blocks. May only lower the process limit. |
| `max_image_artifacts` | number/string | no | `IRONFLOW_LLM_MAX_IMAGE_ARTIFACTS` / `32` | Maximum image-artifact blocks in one request. May only lower the process limit. |
| `azure_endpoint` | string | conditional | `AZURE_OPENAI_ENDPOINT` | Azure endpoint URL |
| `azure_api_version` | string | no | `AZURE_OPENAI_API_VERSION` | Azure API version |
| `azure_chat_deployment` | string | conditional | `AZURE_OPENAI_CHAT_DEPLOYMENT` | Azure deployment for chat mode |
| `azure_responses_deployment` | string | conditional | `AZURE_OPENAI_RESPONSES_DEPLOYMENT` | Azure deployment for responses mode |
| `api_key` | string | conditional | provider env var | API key or auth token |
| `base_url` | string | conditional | provider env var | Base URL for OpenAI-compatible/custom providers |
| `auth_type` | string | no | `"bearer"` | Custom-provider auth type: `bearer`, `api_key`, or `none` |
| `auth_header` | string | no | `"x-api-key"` for `api_key` auth | Header used when `auth_type = "api_key"` |
| `chat_path` | string | no | `"/chat/completions"` | Custom provider endpoint path for chat |
| `responses_path` | string | no | `"/responses"` | Custom provider endpoint path for responses |

`auto` mode will use chat by default and switch to responses only when `responses_input = true`.

## Environment Variable Fallbacks

| Config Key | Environment Variable | Provider |
|------------|---------------------|----------|
| `api_key` | `OPENAI_API_KEY` | openai |
| `base_url` | `OPENAI_BASE_URL` | openai |
| `base_url` | `OPENAI_COMPATIBLE_BASE_URL` | openai_compatible |
| `base_url` | `LLM_BASE_URL` | openai_compatible/custom |
| `azure_endpoint` | `AZURE_OPENAI_ENDPOINT` | azure |
| `azure_api_version` | `AZURE_OPENAI_API_VERSION` | azure |
| `azure_chat_deployment` | `AZURE_OPENAI_CHAT_DEPLOYMENT` | azure |
| `azure_responses_deployment` | `AZURE_OPENAI_RESPONSES_DEPLOYMENT` | azure |
| `api_key` | `AZURE_OPENAI_API_KEY` | azure |
| `max_response_bytes` | `IRONFLOW_LLM_MAX_RESPONSE_BYTES` | all providers |
| `max_image_input_bytes` | `IRONFLOW_LLM_MAX_IMAGE_INPUT_BYTES` | all chat providers |
| `max_image_artifacts` | `IRONFLOW_LLM_MAX_IMAGE_ARTIFACTS` | all chat providers |

## Context Output

- `{output_key}_text` — extracted model response text
- `{output_key}_raw` — raw provider response as JSON
- `{output_key}_model` — model used in request
- `{output_key}_provider` — resolved provider name
- `{output_key}_mode` — selected mode (`chat` or `responses`)
- `{output_key}_status` — HTTP status code
- `{output_key}_success` — `true` on success
- `{output_key}_usage` — token usage section when available
- `{output_key}_tool_calls` — parsed tool call objects (if any)
- `{output_key}_tool_call_needed` — `true` when model returned one or more tool calls
- `{output_key}_tool_call_names` — list of called function names
- `{output_key}_tool_calls_normalized` — provider-neutral tool calls with parsed arguments:
  `{ id, index, type, name, arguments, raw_arguments, raw_call }`

Provider response bodies are streamed with a hard byte cap before JSON parsing. Set `IRONFLOW_LLM_MAX_RESPONSE_BYTES=0` to disable the global cap, or use per-node `max_response_bytes` for a specific trusted workflow.

## Runtime Messages and Image Artifacts

Use `messages_key` when a prior step builds or extends a conversation array.
The selected context value must be an array. Unlike inline `messages`, values
loaded through `messages_key` are used verbatim rather than interpolated, so
provider tool-call IDs and generated text are preserved exactly.

Chat message content arrays may contain an IronFlow-only `image_artifact`
block. It must specify exactly one source:

- `source_key`: a context key containing an artifact descriptor or
  `artifact://sha256/...` URI;
- `artifact`: an artifact descriptor or URI directly embedded in the runtime
  message.

```lua
flow:step("read_page", nodes.read_file({
    path = "/workspace/page-1.png",
    encoding = "artifact",
    mime_type = "image/png",
    output_key = "page"
}))

flow:step("analyze", nodes.llm({
    provider = "custom",
    mode = "chat",
    model = "gemini-3.7-flash",
    base_url = "https://generativelanguage.googleapis.com/v1beta/openai",
    auth_type = "bearer",
    api_key = env("GEMINI_API_KEY"),
    messages = {
        {
            role = "user",
            content = {
                { type = "text", text = "Read this page." },
                {
                    type = "image_artifact",
                    source_key = "page_artifact",
                    detail = "high"
                }
            }
        }
    }
})):depends_on("read_page")
```

IronFlow opens the artifact through the configured integrity-verifying store,
sniffs its actual format, and creates the provider `image_url` data URL only in
the ephemeral HTTP request body. The stored workflow context and node output
retain only the artifact descriptor. Supported formats are PNG, JPEG, WebP,
and GIF. Optional `mime_type` must agree with both descriptor metadata and
detected bytes; optional `detail` is `auto`, `low`, or `high`. Local filesystem
paths are not accepted.

Raw bytes are bounded cumulatively before Base64 expansion by
`IRONFLOW_LLM_MAX_IMAGE_INPUT_BYTES` and image count by
`IRONFLOW_LLM_MAX_IMAGE_ARTIFACTS`. Per-node values can tighten but not raise
those process limits. Artifact blocks are chat-only; Responses mode is
unchanged. The selected provider must support OpenAI-compatible `image_url`
content blocks.

## Examples

### OpenAI Chat (simple)

```lua
flow:step("chat", nodes.llm({
    provider = "openai",
    model = "gpt-5-mini",
    prompt = "Hello",
    temperature = 0.3,
    output_key = "chat"
}))
```

### Azure Chat

```lua
flow:step("chat", nodes.llm({
    provider = "azure",
    mode = "chat",
    model = "gpt-5",
    prompt = "Hello",
    temperature = 0.3,
    output_key = "azure_chat"
}))
```

### OpenAI-compatible Responses

```lua
flow:step("responses", nodes.llm({
    provider = "openai_compatible",
    mode = "responses",
    model = "gpt-5-mini",
    prompt = "Hello",
    output_key = "responses"
}))
```

### Gemini (custom provider)

```lua
flow:step("chat", nodes.llm({
    provider = "custom",
    mode = "chat",
    model = "gemini-3.7-flash",
    prompt = "Hello",
    base_url = "https://generativelanguage.googleapis.com/v1beta/openai",
    auth_type = "bearer",
    api_key = env("GEMINI_API_KEY"),
    output_key = "gemini_chat"
}))
```

### OpenAI response_format: `json_object` and `json_schema`

```lua
flow:step("json_object", nodes.llm({
    provider = "openai",
    model = "gpt-5-mini",
    temperature = 0.0,
    prompt = "Return a JSON object with keys `language` and `topic`.",
    output_key = "openai_json_object",
    extra = {
        response_format = {
            type = "json_object",
        }
    }
}))

flow:step("json_schema", nodes.llm({
    provider = "openai",
    model = "gpt-5-mini",
    temperature = 0.0,
    prompt = "Return JSON with sentiment and confidence.",
    output_key = "openai_json_schema",
    extra = {
        response_format = {
            type = "json_schema",
            json_schema = {
                name = "sentiment_schema",
                strict = true,
                schema = {
                    type = "object",
                    properties = {
                        sentiment = { type = "string", enum = { "positive", "neutral", "negative" } },
                        confidence = { type = "number", minimum = 0, maximum = 1 },
                    },
                    required = { "sentiment", "confidence" },
                    additionalProperties = false,
                },
            },
        }
    }
}))
```

### OpenAI Responses internal tools (web search)

```lua
flow:step("search", nodes.llm({
    provider = "openai",
    mode = "responses",
    model = "gpt-4o-mini",
    prompt = "Use web search to find ...",
    output_key = "search",
    extra = {
        tools = {
            { type = "web_search_preview" }
        },
        tool_choice = "auto"
    }
}))
```

### OpenAI function calling with Lua-defined function tools

```lua
flow:step("ask", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    messages = {
        { role = "user", content = "What is the current weather in Paris?" },
    },
    tools = {
        {
            type = "function",
            function = {
                name = "get_weather",
                description = "Get the current weather for a city.",
                parameters = {
                    type = "object",
                    properties = {
                        city = {
                            type = "string",
                            description = "City name requested by the user.",
                        },
                    },
                    required = { "city" },
                    additionalProperties = false,
                },
            },
        },
    },
    tool_choice = "required",
    output_key = "weather_tool",
}))

-- `weather_tool_tool_calls` contains the raw provider tool call payload:
-- {
--   {
--     id = "call_xxx",
--     type = "function",
--     function = { name = "get_weather", arguments = '{"city":"Paris"}' }
--   }
-- }
--
-- `weather_tool_tool_calls_normalized` contains the easier dispatch shape:
-- {
--   {
--     id = "call_xxx",
--     index = 0,
--     type = "function",
--     name = "get_weather",
--     arguments = { city = "Paris" },
--     raw_arguments = '{"city":"Paris"}',
--     raw_call = { ... }
--   }
-- }
```

`llm` exposes tool-calling details as `{output_key}_tool_calls`, `{output_key}_tool_calls_normalized`, `{output_key}_tool_call_needed`, and `{output_key}_tool_call_names`. Use [`tool_dispatch`](tool_dispatch.md) to execute returned tool calls through mapped subworkflows, then `messages_key` for the next model turn. Bound multi-turn loops with [`repeat_subworkflow`](repeat_subworkflow.md).

`llm` also merges `extra` into the request body as-is for providers that do not yet expose all fields in this table.

For reasoning-style chat models whose names start with `gpt-5`, `o1`, or `o3`, explicit `temperature` is omitted because those providers commonly require default sampling behavior.
