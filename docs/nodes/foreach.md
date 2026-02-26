# `foreach`

Iterate over an array, execute a Lua function per item, and collect results.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key containing the array to iterate over. |
| `transform` | function | yes | — | Lua function called with `(item, index)` for each element. |
| `output_key` | string | no | `"foreach_results"` | Context key where the result array is stored. |
| `filter_nulls` | bool | no | `true` | When `true`, items where the transform returns `nil` are excluded from the results. |

> The function is serialized to bytecode at parse time (same mechanism as function handlers on `flow:step()`).
> The `ctx` table and `env()` function are available as globals inside the transform.
> The Lua environment is sandboxed: `os`, `io`, `debug`, `loadfile`, and `dofile` are removed.

## Context Output

- `{output_key}` (default `foreach_results`) — array of transformed values.
- `{output_key}_count` (default `foreach_results_count`) — number of items in the result array (after filtering).

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
