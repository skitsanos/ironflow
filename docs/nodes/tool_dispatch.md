# `tool_dispatch`

Run one handler subworkflow per LLM tool call and return structured results
plus chat-style tool result messages. Accepts the raw
`{output_key}_tool_calls` or the normalized `{output_key}_tool_calls_normalized`
from [`llm`](llm.md).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the array of raw or normalized tool calls. |
| `tools` | object | Yes | — | Map of tool name to handler `{ flow = "...", input = { ... } }`; `flow` is required. |
| `output_key` | string | No | `"tool_results"` | Prefix for the output keys. |
| `on_error` | string | No | `"fail_fast"` | `"fail_fast"` fails the step on the first failed or unsupported call; `"ignore"` records it and continues. |
| `max_calls` | number | No | `32` | Maximum calls accepted from `source_key`; more fails the step. Must be greater than 0. |

### Tool Handler Mapping

```lua
tools = {
    get_weather = {
        flow = "tool_weather_subworkflow.lua",
        input = {
            city = "arguments.city",
            call_id = "tool_call_id",
            full_call = "call",
            user_id = "ctx.user.id",
            locale = { optional = "ctx.user.locale" },
            units = { literal = "metric" },
        },
    },
}
```

| Mapping value | Meaning |
|---------------|---------|
| `"arguments"`, `"arguments.<path>"` | Parsed arguments, or a path inside them; `null` when missing. |
| `"call"`, `"call.<path>"` | Normalized call object, or a path inside it; `null` when missing. |
| `"tool_name"`, `"tool_call_id"` | Tool name or call ID. |
| `"ctx.<key>[.<path>]"` | Value from the parent context; must exist. |
| any other string | Top-level parent context key; must exist. |
| `{ optional = "<reference>" }` | Any reference above; `null` when it does not resolve. |
| `{ literal = <value> }` | The value as written; nothing inside is resolved. |
| other objects and arrays | Resolved recursively; numbers, booleans and `null` are passed as written. |

Paths are dotted, with zero-based array indices (`ctx.users.0.id`); the whole
path must resolve. A parent reference that does not resolve fails the step
before any handler runs, for every tool named by a call, and `on_error =
"ignore"` does not suppress it. A missing tool-call value reaches the handler as
`json_null`, not `nil`. Only an object with exactly one key, `optional` or
`literal`, is a form. An `input` that is not an object fails the step.

Every handler also receives `tool_call`, `tool_name`, `tool_arguments`,
`tool_call_id` and `tool_call_index` (zero-based). Unmapped parent keys are not
inherited.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | One result entry per call, in call order. |
| `{output_key}_count` | number | Calls processed. |
| `{output_key}_errors` | number | Failed or unsupported calls. |
| `{output_key}_all_succeeded` | boolean | `true` when every call succeeded. |
| `{output_key}_messages` | array | `{ role = "tool", tool_call_id, content }` per call, ready to append to a conversation. |
| `{output_key}_by_id` | object | Result entries keyed by call ID (calls without an ID are omitted). |

## Output Shape

```json
{
  "success": true,
  "id": "call_1",
  "name": "get_weather",
  "arguments": { "city": "Berlin" },
  "flow": "tool_weather",
  "result": { "temp_c": 12 },
  "content": "{\"temp_c\":12}"
}
```

- `result` is the handler's `tool_result_value`, else `tool_result_text`, else its context without `_`-prefixed keys.
- `content` is `result` as text: a string as-is, anything else as JSON.
- `error` is present on failure. An unsupported tool yields `success = false`, `error`, `result = { error }` and no `flow` or `content`.

## Behavior

- Calls run one after another, not concurrently.
- A handler failure error lists the handler's failed tasks, bounded and redacted as in [`subworkflow`](subworkflow.md#behavior).
- A missing flow file or a missing `flow` field fails the step regardless of `on_error`.
- Handler limits, deadline and cancellation follow [`subworkflow`](subworkflow.md#behavior).
- For another model turn, append the assistant message and `{output_key}_messages` to a conversation and pass it to `llm`; wrap the loop in [`repeat_subworkflow`](repeat_subworkflow.md) to bound it.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_SUBWORKFLOW_DEPTH` | `16` | Child nesting depth across all composition nodes. |
| `IRONFLOW_MAX_LIVE_CHILD_RUNS` | `4096` | Child runs alive at once in the process. |

## Examples

### Dispatch tool calls from `llm`

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

A credential-free example of every mapping form is
[`tool_dispatch_input_projection.lua`](../../examples/13-ai/tool_dispatch_input_projection.lua).
