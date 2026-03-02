# parallel_subworkflows

Execute multiple subworkflows concurrently and collect their results.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `flows` | array | yes | — | Array of flow configurations to execute in parallel |
| `output_key` | string | no | `"parallel_results"` | Key for the results array in context |
| `on_error` | string | no | `"fail_fast"` | Error handling: `"fail_fast"` (fail on any error) or `"ignore"` (collect all results) |

### Flow Entry Parameters

Each entry in the `flows` array:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `flow` | string | yes | Path to the `.lua` flow file (relative to `_flow_dir`) |
| `input` | object | no | Context mapping — keys are child context keys, values are parent context keys or literals. String values are treated as a parent context key when present, otherwise kept as string literals. |
| `output_key` | string | no | Namespace the child's output under this key in the result entry |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Array of result objects, one per flow (in original order) |
| `{output_key}_count` | number | Total number of flows |
| `{output_key}_errors` | number | Number of flows that failed |
| `{output_key}_all_succeeded` | boolean | `true` if all flows succeeded |

Each result entry contains:

| Key | Type | Description |
|-----|------|-------------|
| `success` | boolean | Whether the subworkflow succeeded |
| `flow` | string | The flow name |
| `error` | string | Error message (only present on failure) |
| *context keys* | any | Child flow output (merged directly or under per-flow `output_key`) |

## Lua Example

```lua
local flow = Flow.new("parallel_workers")

-- Run three flows concurrently
flow:step("run_all", nodes.parallel_subworkflows({
    flows = {
        { flow = "fetch_users.lua", output_key = "users" },
        { flow = "fetch_orders.lua", output_key = "orders" },
        { flow = "fetch_metrics.lua", output_key = "metrics" }
    }
}))

-- Use results from all three
flow:step("summarize", function(ctx)
    local results = ctx.parallel_results
    return {
        all_ok = ctx.parallel_results_all_succeeded,
        count = ctx.parallel_results_count
    }
end):depends_on("run_all")

return flow
```

### With input mapping

```lua
flow:step("process", nodes.parallel_subworkflows({
    flows = {
        { flow = "worker.lua", input = { job_id = "job_1" } },
        { flow = "worker.lua", input = { job_id = "job_2" } },
        { flow = "worker.lua", input = { job_id = "job_3" } }
    },
    on_error = "ignore"
}))
```

### Error handling

```lua
-- fail_fast (default): step fails if any subworkflow fails
flow:step("strict", nodes.parallel_subworkflows({
    flows = { ... }
}))

-- ignore: collect all results, check errors yourself
flow:step("tolerant", nodes.parallel_subworkflows({
    flows = { ... },
    on_error = "ignore"
}))
```
