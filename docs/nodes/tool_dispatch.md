# `tool_dispatch`

Dispatch LLM tool calls to mapped subworkflow handlers and collect tool results.

`tool_dispatch` accepts raw `{output_key}_tool_calls` from `llm` or normalized `{output_key}_tool_calls_normalized`. It executes one subworkflow per tool call, preserves call IDs, and returns both structured results and chat-compatible tool result messages.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | - | Context key containing an array of raw or normalized tool calls. |
| `tools` | object | yes | - | Map from tool name to handler config. Each handler currently supports `flow` and optional `input`. |
| `output_key` | string | no | `"tool_results"` | Context key for dispatch result array. |
| `on_error` | string | no | `"fail_fast"` | Error policy: `"fail_fast"` or `"ignore"`. |
| `max_calls` | number | no | `32` | Maximum number of tool calls accepted from `source_key`. |

## Tool Handler Mapping

Each tool mapping uses a subworkflow:

```lua
tools = {
    get_weather = {
        flow = "tool_weather_subworkflow.lua",
        input = {
            city = "arguments.city",
            call_id = "tool_call_id",
            full_call = "call",
            user_id = "ctx.user.id",
        },
    },
}
```

Input values can reference:

| Reference | Meaning |
|-----------|---------|
| `arguments` | Full parsed argument object. |
| `arguments.city` | Dotted path inside parsed arguments. |
| `call` | Full normalized tool call object. |
| `call.id`, `call.name`, etc. | Dotted path inside the normalized call. |
| `tool_name` | Tool function name. |
| `tool_call_id` | Tool call ID. |
| `ctx.key` | Dotted path from the parent workflow context. |
| any other string | Parent context key if present, otherwise literal string. |

Every child subworkflow also receives:

- `tool_call` — full normalized call object
- `tool_name` — tool function name
- `tool_arguments` — parsed arguments
- `tool_call_id` — provider call ID
- `tool_call_index` — zero-based call index

## Context Output

- `<output_key>` — array of result entries:
  `{ success, id, name, arguments, flow, result, content, error? }`
- `<output_key>_count` — number of tool calls processed
- `<output_key>_errors` — number of failed or unsupported calls
- `<output_key>_all_succeeded` — `true` when every call succeeded
- `<output_key>_messages` — chat-style tool result messages:
  `{ role = "tool", tool_call_id = "...", content = "..." }`
- `<output_key>_by_id` — object keyed by tool call ID

The child subworkflow result is selected from `tool_result_value` first, then `tool_result_text`, then the child context with private keys removed.

`tool_dispatch` handles one set of model-requested calls. For another model
turn, append the assistant tool-call message and `<output_key>_messages` to a
runtime conversation array, then pass it to `llm` with `messages_key`. For a
bounded multi-turn agent, place that sequence in a child flow and control it
with [`repeat_subworkflow`](repeat_subworkflow.md).

## Example

```lua
flow:step("ask", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    output_key = "assistant",
    messages = {
        { role = "user", content = "What is the weather in Berlin?" },
    },
    tools = {
        {
            type = "function",
            ["function"] = {
                name = "get_weather",
                description = "Get weather for a city.",
                parameters = {
                    type = "object",
                    properties = {
                        city = { type = "string" },
                    },
                    required = { "city" },
                    additionalProperties = false,
                },
            },
        },
    },
    tool_choice = "required",
}))

flow:step("run_tools", nodes.tool_dispatch({
    source_key = "assistant_tool_calls_normalized",
    output_key = "tool_results",
    tools = {
        get_weather = {
            flow = "tool_weather_subworkflow.lua",
            input = {
                city = "arguments.city",
            },
        },
    },
})):depends_on("ask")
```
