# `repeat_subworkflow`

Execute one child workflow sequentially until it returns an explicit completion
decision. Each iteration receives static input, a one-based iteration number,
and only the state value returned by the preceding iteration.

Use this node for bounded model/tool turns, pagination, and short polling loops.
It is an in-process composition primitive, not durable suspension or an
exactly-once side-effect mechanism.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `flow` | string | yes | - | Child `.lua` flow, resolved relative to the parent flow directory. |
| `max_iterations` | positive integer | yes | - | Maximum child runs before the step fails. Cannot exceed `IRONFLOW_MAX_REPEAT_ITERATIONS`. |
| `input` | object | no | parent context | Static child input mapping. A string value selects the parent key when that key exists; otherwise it remains a literal. |
| `output_key` | string | no | `"repeat_result"` | Namespace for the final public child context and output metadata. |
| `state_key` | string | no | `"repeat_state"` | Child input key containing the current carried state. |
| `next_state_key` | string | no | `"repeat_next_state"` | Child output key containing the state for the next iteration. |
| `until_key` | string | no | `"repeat_done"` | Child output key containing the required boolean completion decision. |
| `iteration_key` | string | no | `"repeat_iteration"` | Child input key containing the one-based iteration number. |
| `delay_seconds` | number | no | `0` | Delay after an incomplete iteration and before the next child run. |
| `backoff_factor` | number | no | `1` | Delay multiplier after each wait; must be at least `1`. |
| `max_delay_seconds` | number | no | `max(delay_seconds, 60)` | Per-wait delay ceiling, with an absolute maximum of 3600 seconds. |

`state_key`, `next_state_key`, `until_key`, and `iteration_key` must be
distinct. Explicit `input` cannot set the latter three runtime-owned keys.

## Child Contract

For every iteration the child must return a boolean under `until_key`:

- `true` completes the node. If `next_state_key` is present, that value is the
  final state; otherwise the current state is retained.
- `false` continues and requires `next_state_key`. Only that value is carried
  into the next iteration.

Other child output is not carried forward. Failed child runs, missing or
non-boolean completion values, a missing next state, and exhaustion of
`max_iterations` fail the parent step without publishing a partial successful
result.

All registered composition nodes are available inside the child. Parent step
timeouts and cancellation remain authoritative over active child runs and
inter-iteration delays.

## Context Output

For `output_key = "result"`:

| Key | Type | Description |
|-----|------|-------------|
| `result` | object | Final child context with `_`-prefixed private keys removed. |
| `result_state` | any | Final carried state, or `null` when no state was supplied. |
| `result_iterations` | integer | Number of child runs performed. |
| `result_completed` | boolean | Always `true` on successful node completion. |
| `result_flow` | string | Child flow name. |

Iteration history is intentionally not retained.

## Example

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

The complete runnable pair is in
[`examples/11-subworkflow/repeat_subworkflow.lua`](../../examples/11-subworkflow/repeat_subworkflow.lua).

## Operational Bounds

`IRONFLOW_MAX_REPEAT_ITERATIONS` defaults to `128` and must be between `1` and
the compiled absolute ceiling of `1024`. A node may choose a lower
`max_iterations`, but cannot raise the process limit. The node stores only the
current state and final child context; payload growth inside that state remains
the workflow author's responsibility.
