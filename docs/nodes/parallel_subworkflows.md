# `parallel_subworkflows`

Execute multiple subworkflows concurrently and collect their results.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `flows` | array | conditional | — | Static array of flow configurations to execute in parallel. Use either `flows` or dynamic `flow` + `source_key`. |
| `flow` | string | conditional | — | Dynamic fan-out child flow path. Required when `flows` is omitted. |
| `source_key` | string | conditional | — | Context key containing the runtime array to fan out over. Required when `flows` is omitted. |
| `input` | object | no | — | In dynamic mode, base input mapping applied to every child run. |
| `item_key` | string | no | `"item"` | Dynamic mode child context key that receives the current source item. |
| `index_key` | string | no | `"index"` | Dynamic mode child context key that receives the 1-based source item index. |
| `child_output_key` | string | no | — | Dynamic mode namespace for each child context inside its result entry; cannot be `success`, `flow`, or `error`. |
| `output_key` | string | no | `"parallel_results"` | Key for the results array in context |
| `on_error` | string | no | `"fail_fast"` | Error handling: `"fail_fast"` (fail on any error) or `"ignore"` (collect all results) |
| `max_concurrent` | number | no | CPU count | Maximum child workflows executing at the same time. Hard-capped at `1024`. |

### Flow Entry Parameters

Each entry in the `flows` array:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `flow` | string | yes | Path to the `.lua` flow file (relative to `_flow_dir`) |
| `input` | object | no | Context mapping — keys are child context keys, values are parent context keys or literals. String values are treated as a parent context key when present, otherwise kept as string literals. |
| `output_key` | string | no | Namespace the child's output under this key in the result entry; cannot be `success`, `flow`, or `error`. |

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
| `flow` | string | The loaded child flow's declared name, or its configured path when loading/execution cannot return a finalized child result |
| `error` | string | Error message (only present on failure) |
| *context keys* | any | Public child context, excluding reserved metadata keys when flattened, or under per-flow `output_key` |

`success`, `flow`, and `error` are reserved at the top level of each result
entry. They describe the actual child execution, never values published or
inherited by the child. Successful entries have no top-level `error` field;
failed entries have the engine's failure description. Aggregate counts and
flags follow those same execution outcomes.

Without a child namespace, same-named child fields are omitted from the result.
To retain them, use a non-reserved per-flow `output_key` (static mode) or
`child_output_key` (dynamic mode), such as `"child"`. Then `result.success`
remains authoritative while `result.child.success`, `result.child.flow`, and
`result.child.error` retain their domain values. Nested fields are not filtered
by these metadata names; the existing private-key and redaction rules still
apply.

Reserved namespace names are configuration errors, rejected at node execution
before any child loads or starts, including an empty dynamic source. They are
not suppressed by `on_error = "ignore"`. The node-level `output_key` names the
results array in the parent context and is not subject to this restriction.
Existing workflows using a reserved child namespace must rename it; workflows
needing flattened domain fields with these names must opt into namespacing.

Both static and dynamic fan-out collect full, redacted live child results with
top-level `_`-prefixed keys removed. `IRONFLOW_MAX_TASK_OUTPUT_BYTES` bounds
persisted task/final inspection snapshots, not the values delivered to parent
steps. The existing `on_error` policy is unchanged: ignoring a failure does not
discard values committed before that failure.

This is an in-process handoff, not durable result recovery. Existing child
execution limits and parent cancellation/timeouts still apply. Large results
remain in memory while collected; prefer artifact references for bulky payloads.

## Example

The offline [`parallel_result_metadata.lua`](../../examples/11-subworkflow/parallel_result_metadata.lua)
example and its [`metadata_child.lua`](../../examples/11-subworkflow/metadata_child.lua)
helper verify successful and failed static/dynamic children with colliding
domain fields, including namespaced preservation.

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

### Dynamic fan-out from context

Use `flow` + `source_key` when the parent workflow discovers work items at runtime. The child flow receives the current item under `item` and its 1-based position under `index` by default.

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

If `ctx.jobs` is:

```json
[
  { "id": "a", "path": "/tmp/a.txt" },
  { "id": "b", "path": "/tmp/b.txt" }
]
```

then `worker.lua` runs twice. Each child context receives `job`, `job_index`, plus any mapped `input` fields. Dynamic mode allows an empty source array and returns an empty results array with `{output_key}_all_succeeded = true`.

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
