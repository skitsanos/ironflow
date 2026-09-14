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

References resolve the **complete** dotted path. Missing roots or fields,
out-of-range array indices, traversal through a scalar, and empty path segments
resolve to JSON `null`, never the parent object. For example, if `user` contains
`{ profile = { name = "Ada" }, private_note = "internal" }`, `ctx.user.id`
selects `null`, while `ctx.user.profile.name` selects `"Ada"`. `ctx.user.` is
malformed and also selects `null`. The same rule applies to `arguments.*` and
`call.*`, including a trailing dot with no field.

Array selectors use zero-based dotted indices such as `ctx.users.0.id`; these
selectors are not `${ctx...}` interpolation and do not use bracket syntax.
Mapping objects and arrays resolve their values recursively. Numbers, booleans,
and JSON null remain unchanged. A value selected from context is copied as-is,
not reinterpreted as another reference.

Missing values do not fail dispatch by themselves. Lua handlers receive the
`json_null` sentinel, not `nil`, and can reject required values explicitly:

```lua
assert(ctx.user_id ~= json_null, "user_id is required")
```

If the handler fails, the existing `on_error` policy applies. To select an entire
object intentionally, use `ctx.user`, `arguments`, or `call`. The legacy bare
string `user` still selects the parent key if present. These explicit selections
are not redacted; avoid copying whole objects that contain private fields.

Every child subworkflow also receives:

- `tool_call` — full normalized call object
- `tool_name` — tool function name
- `tool_arguments` — parsed arguments
- `tool_call_id` — provider call ID
- `tool_call_index` — zero-based call index

Parent context is not automatically inherited beyond mapped values. The default
call metadata still includes the complete arguments, and result entries also
include those arguments. This mapping rule is not a secret filter or an
authorization boundary; do not place credentials in tool arguments. The parent
workflow's own context/history is unchanged.

## Context Output

- `<output_key>` — array of result entries:
  `{ success, id, name, arguments, flow, result, content, error? }`
- `<output_key>_count` — number of tool calls processed
- `<output_key>_errors` — number of failed or unsupported calls
- `<output_key>_all_succeeded` — `true` when every call succeeded
- `<output_key>_messages` — chat-style tool result messages:
  `{ role = "tool", tool_call_id = "...", content = "..." }`
- `<output_key>_by_id` — object keyed by tool call ID

The child subworkflow result is selected from `tool_result_value` first, then
`tool_result_text`, then the child context without top-level `_`-prefixed keys.
This top-level private-key filter does not remove nested `_`-prefixed fields.
Execution-overlay redaction still applies.

Selection uses full, redacted live child values after finalization. The
`IRONFLOW_MAX_TASK_OUTPUT_BYTES` inspection cap does not truncate results,
the by-ID collection, or tool message content passed to later steps. Persisted
CLI/API inspection may still show truncation markers. This in-process handoff
does not add durable result recovery or bypass Lua memory/conversion limits,
parent cancellation/timeouts, or provider request limits. Result collections
and messages consume memory; prefer compact results or artifact references for
large payloads.

`tool_dispatch` handles one set of model-requested calls. For another model
turn, append the assistant tool-call message and `<output_key>_messages` to a
runtime conversation array, then pass it to `llm` with `messages_key`. For a
bounded multi-turn agent, place that sequence in a child flow and control it
with [`repeat_subworkflow`](repeat_subworkflow.md).

## Example

For a credential-free runnable check of nested selection and missing-value
isolation, see [`tool_dispatch_input_projection.lua`](../../examples/13-ai/tool_dispatch_input_projection.lua)
and its [`tool_input_projection_subworkflow.lua`](../../examples/13-ai/tool_input_projection_subworkflow.lua)
handler.

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
