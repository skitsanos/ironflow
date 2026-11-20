# `foreach`

Call a Lua function for each item of a context array and collect the return
values.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the array to iterate over. |
| `transform` | function | Yes | — | Lua function called as `transform(item, index)` with a 1-based `index`. |
| `output_key` | string | No | `"foreach_results"` | Context key for the result array. |
| `filter_nulls` | boolean | No | `true` | Drop items for which the transform returns `nil`. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Transformed values, in source order. |
| `{output_key}_count` | number | Number of values after filtering. |

## Behavior

- A missing `source_key`, a value that is not an array, or a transform error fails the step; the error names the failing item.
- The transform runs in the same sandbox as [`code`](code.md): `ctx`, `env()`, the JSON, base64, logging, UUID and timestamp helpers are available; `os`, `io`, `debug`, `loadfile` and `dofile` are not; `Flow` and `nodes` are not available.
- The transform must be self-contained: captured locals from the enclosing flow are rejected. Declare constants inside the function or read them from `ctx`.
- `ironflow validate` reports undefined globals in the transform; `--strict` makes them failures.
- Step timeouts and cancellation stop the transform between items and during execution.

## Limits

The transform runs under the Lua execution limits (`IRONFLOW_LUA_MAX_INSTRUCTIONS`,
`IRONFLOW_LUA_MAX_SECONDS`, `IRONFLOW_LUA_MAX_MEMORY_BYTES`,
`IRONFLOW_LUA_MAX_PATTERN_STEPS`); see [`code`](code.md#limits).

## Examples

### Transform each item

```lua
local flow = Flow.new("line_items")

flow:step("calc", nodes.foreach({
    source_key = "products",
    output_key = "line_items",
    transform = function(item, index)
        return {
            line = index,
            name = string.upper(item.name),
            total = item.price * item.qty
        }
    end
}))

flow:step("done", nodes.log({
    message = "Calculated ${ctx.line_items_count} line items"
})):depends_on("calc")

return flow
```

### Filter with nil

```lua
-- Return nil to skip items (filtered out by default)
flow:step("admins_only", nodes.foreach({
    source_key = "users",
    output_key = "admin_names",
    transform = function(item)
        if item.role == "admin" then
            return item.name
        end
    end
}))
```
