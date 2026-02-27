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
| `system_prompt` | string | no | — | System message used when building chat `messages` automatically |
| `system` | string | no | — | Alias for `system_prompt` |
| `temperature` | number | no | — | Sampling temperature |
| `max_tokens` | number | no | — | OpenAI chat `max_tokens` (mapped to `max_tokens` for chat and `max_output_tokens` for responses) |
| `max_output_tokens` | number | no | — | Direct `max_output_tokens` passthrough |
| `response_format` | object | no | — | OpenAI-compatible response format override. Useful aliases: `{ type = "json_object" }` or `{ type = "json_schema", json_schema = { ... } }` |
| `extra` | object | no | — | Extra request fields merged into payload |
| `output_key` | string | no | `"llm"` | Prefix for output context keys |
| `timeout` | number | no | `30` | Request timeout in seconds |
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
    model = "gemini-3-flash-preview",
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

-- `weather_tool_tool_calls` contains the tool call payload:
-- {
--   {
--     id = "call_xxx",
--     type = "function",
--     function = { name = "get_weather", arguments = '{"city":"Paris"}' }
--   }
-- }
```

`llm` exposes tool-calling details as `{output_key}_tool_calls`, `{output_key}_tool_call_needed`, and `{output_key}_tool_call_names`.

`llm` also merges `extra` into the request body as-is for providers that do not yet expose all fields in this table.
