# `repeat_subworkflow`

Run one child flow repeatedly, one iteration at a time, until it returns an
explicit completion flag. Only the state value the child returns is carried
into the next iteration.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `flow` | string | Yes | — | Child `.lua` flow, relative to the parent flow's directory. |
| `max_iterations` | integer | Yes | — | Maximum child runs; must be at least 1 and at most `IRONFLOW_MAX_REPEAT_ITERATIONS`. |
| `input` | object | No | parent context | Static child input (see [Input mapping](subworkflow.md#input-mapping)); cannot set `next_state_key`, `until_key` or `iteration_key`. |
| `output_key` | string | No | `"repeat_result"` | Prefix for the output keys. |
| `state_key` | string | No | `"repeat_state"` | Child key holding the current state; its initial value comes from `input` or the parent context. |
| `next_state_key` | string | No | `"repeat_next_state"` | Child output key holding the state for the next iteration. |
| `until_key` | string | No | `"repeat_done"` | Child output key holding the boolean completion flag. |
| `iteration_key` | string | No | `"repeat_iteration"` | Child key holding the 1-based iteration number. |
| `delay_seconds` | number | No | `0` | Wait after an incomplete iteration; must be non-negative. |
| `backoff_factor` | number | No | `1` | Multiplier applied to the wait after each iteration; must be at least `1`. |
| `max_delay_seconds` | number | No | `max(delay_seconds, 60)` | Ceiling for one wait; at least `delay_seconds`, at most `3600`. |

The four key names must be distinct non-empty strings.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | object | Final child context, without `_`-prefixed keys. |
| `{output_key}_state` | any | Final state, or `null` when no state was ever supplied. |
| `{output_key}_iterations` | integer | Number of child runs. |
| `{output_key}_completed` | boolean | Always `true` on success. |
| `{output_key}_flow` | string | Child flow name. |

## Behavior

- Every iteration must return a boolean under `until_key`; a missing or non-boolean value fails the step.
- `true` completes the node; the final state is `next_state_key` if returned, otherwise the current state.
- `false` requires `next_state_key`; only that value is carried forward. Other child output is dropped.
- A failed child run, or reaching `max_iterations` without completion, fails the step; no partial result is published. The error names the child, the iteration and its failed tasks.
- A string mapping that names no parent key fails the step before the child is loaded; use `{ optional = "key" }` for an initial state that may be absent.
- Child limits, deadline and cancellation follow [`subworkflow`](subworkflow.md#behavior); a step timeout also ends a pending delay.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_REPEAT_ITERATIONS` | `128` | Highest allowed `max_iterations`; valid range 1 to 1024, checked at startup. |
| `IRONFLOW_MAX_SUBWORKFLOW_DEPTH` | `16` | Child nesting depth across all composition nodes. |
| `IRONFLOW_MAX_LIVE_CHILD_RUNS` | `4096` | Child runs alive at once in the process. |

## Examples

### Bounded counter

Parent flow:

```lua
local flow = Flow.new("bounded_counter")

flow:step("initialize", function()
    return { initial = { value = 0 }, target = 3 }
end)

flow:step("count", nodes.repeat_subworkflow({
    flow = "counter_child.lua",
    input = {
        repeat_state = "initial",
        target = "target"
    },
    max_iterations = 5,
    output_key = "counter"
})):depends_on("initialize")

return flow
```

Child flow (`counter_child.lua`):

```lua
local flow = Flow.new("counter_child")

flow:step("advance", function(ctx)
    local current = ctx.repeat_state or { value = 0 }
    local next_state = { value = current.value + 1 }
    return {
        repeat_next_state = next_state,
        repeat_done = next_state.value >= ctx.target,
        value = next_state.value
    }
end)

return flow
```

The runnable pair is
[`examples/11-subworkflow/repeat_subworkflow.lua`](../../examples/11-subworkflow/repeat_subworkflow.lua).
