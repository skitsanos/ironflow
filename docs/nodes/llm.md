# `llm`

Run a chat-style request against OpenAI, OpenAI-compatible, Azure, or custom endpoints using one node and one consistent output shape.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `provider` | string | no | `"openai"` | Provider backend: `"openai"`, `"openai_compatible"`, `"azure"`, `"custom"` |
| `mode` | string | no | `"chat"` | Request mode: `"chat"`, `"responses"`, or `"auto"` |
| `model` | string | no | provider-dependent | Model name (for Azure, defaults to deployment when available) |
| `prompt` | string | no | — | Direct prompt text for user content |
| `input_key` | string | no | `"prompt"` | Context key for prompt text when `prompt` is not set |
| `messages` | array | no | — | Chat-style message objects (`role`, `content`) for chat mode |
| `system_prompt` | string | no | — | System message used when building chat `messages` automatically |
| `system` | string | no | — | Alias for `system_prompt` |
| `temperature` | number | no | — | Sampling temperature |
| `max_tokens` | number | no | — | OpenAI chat `max_tokens` (mapped to `max_tokens` for chat and `max_output_tokens` for responses) |
| `max_output_tokens` | number | no | — | Direct `max_output_tokens` passthrough |
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
