# `subworkflow`

Load and execute another `.lua` flow as a reusable module.

The subworkflow node allows you to compose workflows by calling one flow from another. The child flow is resolved relative to the parent flow's directory (via the injected `_flow_dir` context key), making it easy to organize related flows in the same folder.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `flow` | string | yes | — | Path to the `.lua` flow file to execute. Resolved relative to the parent flow's directory. |
| `wait` | bool | no | `true` | When `true`, the parent blocks until the subworkflow completes. When `false`, the subworkflow is launched in the background (fire-and-forget). |
| `input` | object | no | `nil` | Key mapping from parent context to child context. Each entry maps `child_key = "parent_key"`. |
| `output_key` | string | no | `nil` | If set, the child's output context is namespaced under this key instead of being merged directly into the parent context. Recommended for parallel subworkflows. |
| `on_error` | string | no | see below | `"fail_fast"` fails the parent step when the child run fails; `"ignore"` continues with the child's partial context. |

## Error Handling

A failed child does **not** always fail the parent step. Set `on_error`
explicitly to say which you want:

| `on_error` | Behaviour when the child run fails |
|---|---|
| `"fail_fast"` | The parent step returns an error. |
| `"ignore"` | The parent step succeeds; branch on `subworkflow_success` (or `{output_key}_success`) and read `subworkflow_error` (also `{output_key}_error` when namespaced) for the reason. |

**When `on_error` is omitted, `output_key` decides it** — the historical
behaviour, kept for compatibility:

- no `output_key` → behaves as `fail_fast`
- `output_key` set → behaves as `ignore`

That coupling is easy to miss: adding `output_key` purely to namespace a
child's output also silently turns off error propagation, and a downstream step
then reads an empty value and continues. If a failed child should stop the
flow, say so with `on_error = "fail_fast"` rather than relying on the absence
of `output_key`.

A tolerated failure is logged at `WARN` by the parent (the child's own run
records the underlying error).

Failure descriptions include the child's status and unresolved task errors,
for example `Subworkflow 'child' finished with status: failed: task 'parse': ...boom`.
The same description is used for `fail_fast`, `subworkflow_error`, and
`{output_key}_error`. Errors resolved by a successful recovery handler are not
included, even though the original task remains failed in inspection history.

The live summary contains at most eight failures in task-name order. Each
task name is limited to 128 UTF-8 bytes and each error to 768 UTF-8 bytes,
including any `...[truncated]` marker; omitted failures are counted. The task
summary stays below 8 KiB, independent of the inspection output cap. Credential
and execution-overlay redaction run before truncation. It carries existing
task diagnostics, not additional provider response bodies or child context.
Cancellation or missing task details retain the status-only description.
No context/history reload is used to reconstruct failure details.

```lua
-- Namespace the output AND still fail the step if the child fails.
flow:step("score", nodes.subworkflow({
    flow = "score_one.lua",
    input = { id = "record_id" },
    output_key = "score",
    on_error = "fail_fast"
}))

-- Tolerate a failure and branch on it yourself.
flow:step("enrich", nodes.subworkflow({
    flow = "optional_enrichment.lua",
    output_key = "extra",
    on_error = "ignore"
}))

flow:step("check", function(ctx)
    if not ctx.extra_success then
        log("enrichment unavailable: " .. tostring(ctx.extra_error))
    end
    return { enriched = ctx.extra_success == true }
end):depends_on("enrich")
```

## Context Injection

The engine automatically injects `_flow_dir` into the context when running a flow from a file. This is the directory containing the parent flow script and is used by the subworkflow node to resolve relative `flow` paths.

## Context Output

When `wait = true` (default):

- Public child context keys (including inherited input, excluding `_`-prefixed keys) are merged into the parent context or namespaced under `output_key`.
- `subworkflow_name` — the name of the executed subworkflow.
- `subworkflow_success` — `true` when the child run succeeded. Always present, so the outcome is checkable even without `output_key`.
- `subworkflow_error` — a string holding the status and bounded, redacted unresolved task-error summary when the child did not succeed; JSON `null` when it succeeded. Always present, with or without `output_key`.
- `{output_key}_success` — same flag, namespaced. Only when `output_key` is set.
- `{output_key}_error` — the same value as `subworkflow_error` (string on failure, `null` on success). Only when `output_key` is set.

Branch on `subworkflow_success`, not on the presence of the error key. Node
outputs merge into the run context by key and cannot remove keys, so the error
slot is written on every call: a later successful `subworkflow` step overwrites
an earlier tolerated failure's text with `null` instead of leaving it beside
`subworkflow_success = true`. In Lua handlers and `code` steps a JSON `null`
arrives as the `json_null` sentinel, which is not `nil` and is truthy;
`if ctx.subworkflow_error then` therefore does not detect a failure, while
`type(ctx.subworkflow_error) == "string"` or `ctx.subworkflow_error ~= json_null`
does. `${ctx.subworkflow_error}` interpolation renders `null` as an empty string.

Without `output_key`, the child's own `subworkflow_name`, `subworkflow_success`,
`subworkflow_error`, and `subworkflow_async` keys (describing a grandchild it
ran) are not merged into the parent; the parent's keys describe its direct
child only. With `output_key`, the child's full public context, including those
keys, stays under `output_key`.

The subworkflow's returned map follows the normal phase collision contract: a
later-declared parallel parent step wins duplicate keys. Without `output_key`,
the child returns a broad flattened context that can also contain inherited
input keys, so namespacing is the safest composition boundary when parallel
results must coexist.

Waiting composition receives full, redacted child values directly from the live
execution after finalization, not from persisted inspection snapshots.
`IRONFLOW_MAX_TASK_OUTPUT_BYTES` may replace large values in CLI/API inspection
with truncation markers, but does not truncate the values consumed by parent
steps. With `on_error = "ignore"`, a failed child's prior committed values are
also kept in full while its success flags remain false. Infrastructure/finalization errors
still fail the step; they are not successful partial child results.

This handoff is in-process, not a durable result-recovery API. Existing Lua
memory/conversion limits and parent cancellation/timeouts still apply. Large
values consume memory; use artifact references for bulky payloads rather than
treating the inspection cap as an execution-memory limit.

When `wait = false`:

- `subworkflow_name` — the name of the subworkflow that was launched.
- `subworkflow_async` — set to `true`, indicating the subworkflow is running in the background.

## Examples

[`if135_child_error_details.lua`](../../examples/11-subworkflow/if135_child_error_details.lua)
and its [`if135_error_child.lua`](../../examples/11-subworkflow/if135_error_child.lua)
helper form a self-contained offline example of tolerated failures. The parent
asserts task names and error messages for namespaced, unnamespaced, and parallel
calls, retains partial child context, and checks a successful child's result.
Run the parent with a build containing IF-135; it should finish successfully
with `child_error_details_verified = true`. No external service or credential
is required.

[`live_child_results.lua`](../../examples/11-subworkflow/live_child_results.lua)
and its helper verify a 3 MiB result in a downstream parent step. The final CLI
context shows a truncation marker under `child` at the default inspection cap,
alongside `child_bytes = 3145728` and `live_result_verified = true`.

### Basic usage

Call a helper flow and use its output:

```lua
local flow = Flow.new("basic_subworkflow")

flow:step("set_name", nodes.code({
    source = [[
        return { name = "IronFlow" }
    ]]
}))

flow:step("call_greet", nodes.subworkflow({
    flow = "greet.lua",
    input = {
        name = "name"   -- map parent "name" → child "name"
    }
})):depends_on("set_name")

flow:step("show_result", nodes.log({
    message = "Subworkflow returned: ${ctx.greeting}",
    level = "info"
})):depends_on("call_greet")

return flow
```

### Fire-and-forget

Launch a subworkflow in the background without waiting for it to finish:

```lua
flow:step("async_greet", nodes.subworkflow({
    flow = "greet.lua",
    wait = false,
    input = {
        name = "name"
    }
})):depends_on("prepare")
```

When `wait = false`, the parent step completes immediately with `subworkflow_async = true` in the context.

### Input mapping

Map specific parent context keys into the child flow's context:

```lua
flow:step("call_child", nodes.subworkflow({
    flow = "process_order.lua",
    input = {
        order_id = "current_order_id",   -- child sees ctx.order_id
        customer = "user_info"            -- child sees ctx.customer
    }
}))
```

### Output namespacing with output_key

Avoid key collisions by namespacing the child's output:

```lua
flow:step("call_child", nodes.subworkflow({
    flow = "greet.lua",
    input = { name = "name" },
    output_key = "greet_result"
})):depends_on("set_name")

-- Access the child's output under ctx.greet_result.greeting
flow:step("show", nodes.log({
    message = "Result: ${ctx.greet_result.greeting}"
})):depends_on("call_child")
```

### Reusable helper flow (greet.lua)

```lua
-- greet.lua — A simple reusable subworkflow
-- Expects "name" in context, outputs a greeting message.

local flow = Flow.new("greet")

flow:step("build_greeting", nodes.code({
    source = [[
        local name = ctx.name or "World"
        return { greeting = "Hello, " .. name .. "!" }
    ]]
}))

return flow
```
