# `parallel_subworkflows`

Run several child flows concurrently and collect one result entry per child,
either from a static `flows` list or by fanning one flow out over a context
array.

## Parameters

Use either `flows` (static) or `flow` + `source_key` (dynamic).

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `flows` | array | Static mode | — | Non-empty list of flow entries (see below). |
| `flow` | string | Dynamic mode | — | Child flow path, run once per source item. |
| `source_key` | string | Dynamic mode | — | Context key holding the array to fan out over; may be empty. |
| `input` | object | No | — | Dynamic mode: mapping applied to every child (see [Input mapping](subworkflow.md#input-mapping)). |
| `item_key` | string | No | `"item"` | Dynamic mode: child key that receives the current item, as a literal value. |
| `index_key` | string | No | `"index"` | Dynamic mode: child key that receives the 1-based item index. |
| `child_output_key` | string | No | — | Dynamic mode: namespace for each child's context inside its result entry. |
| `output_key` | string | No | `"parallel_results"` | Context key for the results array. |
| `on_error` | string | No | `"fail_fast"` | `"fail_fast"` fails the step if any child fails; `"ignore"` returns all results. |
| `max_concurrent` | number | No | host CPU count | Children running at once; must be greater than 0, capped at `1024`. |

Each entry of `flows`:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `flow` | string | Yes | Child flow path, relative to the parent flow's directory. |
| `input` | object | No | Mapping for this child (see [Input mapping](subworkflow.md#input-mapping)); without it the child inherits the parent context. |
| `output_key` | string | No | Namespace for this child's context inside its result entry. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | One result entry per child, in input order. |
| `{output_key}_count` | number | Number of children. |
| `{output_key}_errors` | number | Number of failed children. |
| `{output_key}_all_succeeded` | boolean | `true` when no child failed. |

## Output Shape

```json
[
  { "success": true, "flow": "worker", "result": { "status": "done" } },
  { "success": false, "flow": "worker", "error": "Subworkflow 'worker' (index 1) finished with status: failed: task 'parse': ..." }
]
```

- `success`, `flow` (the child's declared name, or its configured path when it could not load) and `error` (failures only) describe the child run.
- Without a namespace, the child's public context is flattened into the entry and child keys named `success`, `flow` or `error` are dropped; use `output_key` / `child_output_key` to keep them.

## Behavior

- `success`, `flow` and `error` are reserved: using one as `output_key` in an entry or as `child_output_key` fails the step before any child starts.
- Every `input` mapping and every flow path is checked before any child starts; a missing parent key or flow file fails the step and no sibling runs. `on_error = "ignore"` does not suppress these.
- `fail_fast` waits for all children, then fails with each failed child's error in input order. It does not stop running siblings.
- Dynamic mode never inherits unmapped parent keys. Items are passed as written, without key lookup: an item `"customer"` is the string, not `ctx.customer`.
- The injected item and index override same-named `input` entries.
- A child's flow is loaded and its context built only when it takes a concurrency slot.
- Child limits, deadline and cancellation follow [`subworkflow`](subworkflow.md#behavior); the error text uses the same bounded summary.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_PARALLEL_SUBWORKFLOWS` | `1024` | Children one call may start; a longer `flows` or source array fails the step before any child starts. |
| `IRONFLOW_MAX_SUBWORKFLOW_DEPTH` | `16` | Child nesting depth across all composition nodes. |
| `IRONFLOW_MAX_LIVE_CHILD_RUNS` | `4096` | Child runs alive at once in the process; nested fan-out multiplies `max_concurrent` per level. |

## Examples

### Static fan-out

```lua
local flow = Flow.new("parallel_workers")

-- Run three flows concurrently
flow:step("run_all", nodes.parallel_subworkflows({
    flows = {
        { flow = "fetch_users.lua", output_key = "users" },
        { flow = "fetch_orders.lua", output_key = "orders" },
        { flow = "fetch_metrics.lua", output_key = "metrics" }
    },
    max_concurrent = 3
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

### Input mapping per entry

```lua
flow:step("process", nodes.parallel_subworkflows({
    flows = {
        { flow = "worker.lua", input = { job_id = { literal = "job_1" } } },
        { flow = "worker.lua", input = { job_id = { literal = "job_2" } } },
        { flow = "worker.lua", input = { job_id = "current_job_id" } }
    },
    on_error = "ignore"
}))
```

### Dynamic fan-out from context

```lua
flow:step("run_jobs", nodes.parallel_subworkflows({
    flow = "worker.lua",
    source_key = "jobs",
    input = {
        run_id = "parent_run_id",
    },
    item_key = "job",
    index_key = "job_index",
    child_output_key = "result",
    max_concurrent = 5,
    output_key = "job_results",
}))
```
