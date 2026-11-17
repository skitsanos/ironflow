# `subworkflow`

Load and run another `.lua` flow as a child of the current step. The child's
public context comes back into the parent context, merged or namespaced.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `flow` | string | Yes | — | Path to the child `.lua` file; a relative path resolves against the parent flow's directory (`_flow_dir`). |
| `wait` | boolean | No | `true` | `true` runs the child to completion; `false` launches it in the background. |
| `input` | object | No | — | Builds the child context from parent keys (see [Input mapping](#input-mapping)); without it the child inherits the whole parent context. |
| `output_key` | string | No | — | Namespace the child's context under this key instead of merging it. |
| `on_error` | string | No | see Behavior | `"fail_fast"` fails the step when the child fails; `"ignore"` continues with the child's partial context. |

### Input mapping

Each entry is `child_key = <mapping value>`. `subworkflow`,
[`parallel_subworkflows`](parallel_subworkflows.md) and
[`repeat_subworkflow`](repeat_subworkflow.md) share this contract.

| Mapping value | Meaning |
|---|---|
| `"parent_key"` | The top-level parent context key. It must exist (a key holding `null` exists), otherwise the step fails. |
| `{ optional = "parent_key" }` | The parent key when it exists; otherwise the child key is omitted. Must be a non-empty string. |
| `{ literal = <value> }` | The value as written, of any type. Use it for a literal string. |
| number, boolean, array, other object | Passed as written; strings nested inside are not looked up. |

Only an object with exactly one key, `optional` or `literal`, is a form; to pass
such an object as data, wrap it: `{ literal = { literal = 1 } }`. An `input` that
is not an object fails the step. Mapping errors are raised before the child is
loaded and are not suppressed by `on_error = "ignore"`.
[`input_mapping_forms.lua`](../../examples/11-subworkflow/input_mapping_forms.lua)
runs every form.

## Context Output

With `wait = true`:

| Key | Type | Description |
|-----|------|-------------|
| child keys | any | The child's public context (keys not starting with `_`), merged into the parent when `output_key` is unset. |
| `{output_key}` | object | The child's public context, when `output_key` is set. |
| `subworkflow_name` | string | Name of the child flow. |
| `subworkflow_success` | boolean | `true` when the child run succeeded. |
| `subworkflow_error` | string or null | Child status and unresolved task errors on failure; `null` on success. |
| `{output_key}_success` | boolean | Same as `subworkflow_success`, when `output_key` is set. |
| `{output_key}_error` | string or null | Same as `subworkflow_error`, when `output_key` is set. |

With `wait = false`: `subworkflow_name` (string) and `subworkflow_async` (`true`).

## Behavior

- When `on_error` is omitted, `output_key` decides it: no `output_key` behaves as `fail_fast`, `output_key` set behaves as `ignore`. Set `on_error` explicitly when you namespace output but still want failures to stop the flow.
- Any other `on_error` value fails the step.
- The error text (`Subworkflow 'child' finished with status: failed: task 'parse': ...`) lists at most eight unresolved task errors, truncated and redacted.
- Branch on `subworkflow_success`: the error key is always written, and in Lua a JSON `null` is the truthy `json_null`.
- Without `output_key`, the child's own `subworkflow_*` keys are not merged into the parent.
- Parent steps receive child values in full; `IRONFLOW_MAX_TASK_OUTPUT_BYTES` only truncates inspection.
- With `wait = false` the child is still resolved, mapped and loaded first, so a missing or invalid child fails the step. Later failures are only logged.
- A waiting child shares the parent step's deadline and is cancelled with it; the step ends only after the child stops. A detached child inherits no deadline.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_SUBWORKFLOW_DEPTH` | `16` | Child levels below the top-level run across all composition nodes; a deeper child fails its step before loading, even with `on_error = "ignore"`. |
| `IRONFLOW_MAX_LIVE_CHILD_RUNS` | `4096` | Child runs alive at once in the process; a child with no free slot fails instead of waiting. |
| `IRONFLOW_MAX_DETACHED_SUBWORKFLOWS` | `64` | Detached (`wait = false`) children running at once; over the limit the step fails. |

`ironflow validate` reports recursion between child flows named with a literal
path; see [Child flows](../CLI_REFERENCE.md#child-flows).

## Examples

### Basic usage

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

### Namespaced output with explicit error policy

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
