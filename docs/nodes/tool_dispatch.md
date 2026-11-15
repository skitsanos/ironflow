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
            locale = { optional = "ctx.user.locale" },
            units = { literal = "metric" },
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
| `ctx.key` | Dotted path from the parent workflow context. Must exist. |
| any other string | Top-level parent context key. Must exist. |
| `{ optional = "<reference>" }` | Any reference above; JSON `null` when it does not resolve. |
| `{ literal = <value> }` | The value as written, of any type. Use it for a literal string. |

References resolve the **complete** dotted path and never fall back to the
parent object. What a reference that does not resolve means depends on who
supplies the value:

- **Parent context** (`ctx.<path>` and bare keys): the flow author controls
  it, so a missing root or field, an out-of-range array index, traversal
  through a scalar or an empty path segment **fails the step** with
  `tool_dispatch: tool '<name>': input '<entry>' references a parent context
  value that does not exist; ...`. `<entry>` is the dotted position inside
  `input`, for example `payload.user_id`; the configured string and context
  values are never printed. A key or field holding JSON `null` exists.
- **Tool call** (`arguments.*`, `call.*`, `tool_name`, `tool_call_id`): the
  model supplies it, so a missing field resolves to JSON `null` and the
  handler decides.

For example, if `user` contains `{ profile = { name = "Ada" }, private_note =
"internal" }`, `ctx.user.profile.name` selects `"Ada"`, `ctx.user.id` and the
malformed `ctx.user.` fail the step, and `{ optional = "ctx.user.id" }`
selects `null`.

The mappings of every tool named by the calls in `source_key` are checked
before the first handler runs, so a failing mapping starts no handler, even
when the call that uses it is not the first. It is a step error, not a
per-call result, and `on_error = "ignore"` does not suppress it. A mapping
for a tool that no call names is not checked. In earlier releases a missing
`ctx.` path resolved to `null` and a bare string that named no parent key was
passed as literal text.

Only an object with exactly one key, `optional` or `literal`, is a form;
wrap such an object in `literal` to pass it as data. `optional` must be a
non-empty string, and a handler `input` that is not an object is rejected.

Array selectors use zero-based dotted indices such as `ctx.users.0.id`; these
selectors are not `${ctx...}` interpolation and do not use bracket syntax.
Mapping objects and arrays resolve their values recursively. Numbers, booleans,
and JSON null remain unchanged. A value selected from context is copied as-is,
not reinterpreted as another reference, and a `literal` value is not resolved.

A missing tool argument or `optional` reference does not fail dispatch by
itself. Lua handlers receive the `json_null` sentinel, not `nil`, and can
reject required values explicitly:

```lua
assert(ctx.user_id ~= json_null, "user_id is required")
```

If the handler fails, the existing `on_error` policy applies. To select an entire
object intentionally, use `ctx.user`, `arguments`, or `call`. The bare
string `user` also selects the parent key. These explicit selections
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

When a child fails, its result entry's `error` and the `fail_fast` error include
the bounded, redacted unresolved task reasons described under
[`subworkflow` error handling](subworkflow.md#error-handling). Successfully
recovered errors are excluded. Cancellation or missing details retain the
status-only fallback. The selected `result`, `content`, and tool-message content
still come from child context; failure details do not replace those values.

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

## Operational Bounds

`max_calls` bounds the calls one step dispatches; they run one after another.
Each handler flow counts towards the
[child workflow nesting limit](subworkflow.md#nesting-limit-and-waiting)
(`IRONFLOW_MAX_SUBWORKFLOW_DEPTH`, default `16`). A handler that reaches
`tool_dispatch` again past that depth fails the step with `tool_dispatch:
child workflow nesting depth 17 exceeds the limit of 16
(IRONFLOW_MAX_SUBWORKFLOW_DEPTH); check for a flow that calls itself`. This is
a step error raised before any call is dispatched, not a per-call result, and
`on_error = "ignore"` does not suppress it. When the step times out or its run
is cancelled, the active handler is cancelled and the step does not end, start
its own `on_error` handler or release its admission until that handler run has
stopped, tracked blocking workers included.

## Example

For a credential-free runnable check of nested selection, the `optional` and
`literal` forms and missing-value isolation, see [`tool_dispatch_input_projection.lua`](../../examples/13-ai/tool_dispatch_input_projection.lua)
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
