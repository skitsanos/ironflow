# `llm`

Send a Chat Completions or Responses request to OpenAI, Azure OpenAI or an OpenAI-compatible endpoint.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `provider` | string | No | `"openai"` | `openai`, `openai_compatible` (alias `compatible`), `azure` or `custom`. |
| `mode` | string | No | `"chat"` | `chat`, `responses` or `auto` (`responses` only with `responses_input = true` and no `messages`). |
| `model` | string | No | `"gpt-5-mini"` | Model; else `chat_model`/`responses_model` or the Azure deployment. |
| `prompt` | string | No | — | User prompt; interpolated. |
| `input_key` | string | No | — | Context key holding the prompt, when `prompt` is absent. |
| `messages` | array | No | — | Chat messages; interpolated. |
| `messages_key` | string | No | — | Context key holding messages, used verbatim. |
| `system_prompt` | string | No | — | System prompt; alias `system`. |
| `temperature` | number | No | — | Not sent for models starting with `gpt-5`, `o1` or `o3`. |
| `max_tokens` | number | No | — | Output-token limit. |
| `max_output_tokens` | number | No | — | Output-token limit. |
| `tools` | array | No | — | OpenAI-style tool definitions. |
| `tool_choice` | string/object | No | — | Tool selection. |
| `response_format` | object | No | — | `json_object` or `json_schema` format. |
| `extra` | object | No | — | Request fields merged last; they win. |
| `output_key` | string | No | `"llm"` | Prefix for output keys. |
| `timeout` | number | No | `30` | Seconds. |
| `max_response_bytes` | number | No | limit | Response cap; `0` uses the limit. |
| `max_image_input_bytes` | number | No | limit | Lowers the image byte limit. |
| `max_image_artifacts` | number | No | limit | Lowers the image count limit. |

One of `prompt`, `input_key`, `messages` or `messages_key` is required; the last two are mutually exclusive.

### Provider settings

The endpoint and, except for `custom`, the credential are required.

| `provider` | Endpoint | Credential |
|------------|----------|------------|
| `openai` | `base_url` or `OPENAI_BASE_URL`, default `https://api.openai.com/v1` | `api_key` or `OPENAI_API_KEY` (Bearer) |
| `openai_compatible` | `base_url`, `OPENAI_COMPATIBLE_BASE_URL` or `LLM_BASE_URL` | `api_key` or `OPENAI_API_KEY` (Bearer) |
| `azure` | `azure_endpoint`; `azure_chat_deployment` or `azure_responses_deployment` (either serves both modes); `azure_api_version` (default `2024-08-01-preview`). `azure_<name>` falls back to `AZURE_OPENAI_<NAME>` | `api_key` or `AZURE_OPENAI_API_KEY` (`api-key` header) |
| `custom` | `base_url` or `LLM_BASE_URL`, plus `chat_path`/`responses_path` or `path` (default `/chat/completions`, `/responses`); an absolute path replaces `base_url` | `api_key` (no environment fallback) per `auth_type`: `bearer` (default), `api_key` (header `auth_header`, default `x-api-key`) or `none` |

### Chat and Responses parameters

| Parameter | Chat mode | Responses mode |
|-----------|-----------|----------------|
| `prompt` / `input_key` | user message | `input` |
| `system_prompt` / `system` | system message | `instructions` |
| `messages` / `messages_key` | `messages` | fails the step |
| `max_tokens` | `max_tokens` | `max_output_tokens` |
| `max_output_tokens` | `max_completion_tokens` | `max_output_tokens` |
| `tools`, `tool_choice`, `response_format` | sent as given | fail the step; use `extra` |

Responses mode cannot return tool results to the model; run tool loops in chat mode with [`tool_dispatch`](tool_dispatch.md).

### Image artifacts

A chat message `content` array may hold an `image_artifact` block with one of `source_key` or `artifact` (URI or descriptor, never a path), optional `mime_type` (must match the bytes) and `detail` (`auto`, `low`, `high`). PNG, JPEG, WebP and GIF become `image_url` data URLs:

```lua
{ type = "image_artifact", source_key = "page_artifact", detail = "high" }
```

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_text` | string | Reply, or `<tool call requested>` |
| `{output_key}_raw` | object | Provider response JSON |
| `{output_key}_model` | string | Model sent |
| `{output_key}_provider` | string | Provider name |
| `{output_key}_mode` | string | `chat` or `responses` |
| `{output_key}_status` | number | HTTP status |
| `{output_key}_success` | boolean | `true` |
| `{output_key}_usage` | object | Token usage, when returned |
| `{output_key}_tool_calls` | array | Provider tool calls, possibly empty |
| `{output_key}_tool_call_needed` | boolean | Any tool call returned |
| `{output_key}_tool_call_names` | array | Called function names |
| `{output_key}_tool_calls_normalized` | array | `{ id, index, type, name, arguments, raw_arguments, raw_call }` |

## Behavior

- An unknown `provider`, `mode` or `auth_type` fails the step before any request.
- A non-2xx response fails the step with the status and redacted body.
- A 2xx reply with neither text nor a tool call fails the step.
- The operator's [outbound destination policy](../SECURITY.md#outbound-destinations) applies to the resolved URL.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_LLM_MAX_RESPONSE_BYTES` | `26214400` | Response body cap; `0` disables it |
| `IRONFLOW_LLM_MAX_IMAGE_INPUT_BYTES` | `52428800` | Image-artifact bytes per request |
| `IRONFLOW_LLM_MAX_IMAGE_ARTIFACTS` | `32` | Image blocks per request |

## Examples

### OpenAI chat

```lua
flow:step("chat", nodes.llm({
    provider = "openai",
    model = "gpt-5-mini",
    prompt = "Hello",
    output_key = "chat"
}))
```

### Responses web search through `extra`

```lua
flow:step("search", nodes.llm({
    provider = "openai",
    mode = "responses",
    model = "gpt-4o-mini",
    prompt = "Use web search to find ...",
    output_key = "search",
    extra = {
        tools = { { type = "web_search_preview" } },
        tool_choice = "auto"
    }
}))
```
