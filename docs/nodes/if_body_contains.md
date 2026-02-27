# `if_body_contains`

Route execution based on whether a context value contains a text pattern.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | -- | Context key to inspect (supports dotted paths, for example `resp.body`) |
| `pattern` | string | Yes | -- | Pattern to search for |
| `true_route` | string | No | `true` | Route name when the pattern is found |
| `false_route` | string | No | `false` | Route name when the pattern is missing |
| `case_sensitive` | bool | No | `true` | Case-sensitive matching |
| `required` | bool | No | `false` | If `true`, missing `source_key` fails the node |

## Context Output

- `_route_{step_name}` — selected route name
- `_contains_{step_name}` — boolean match result

`{step_name}` defaults to `"if_body_contains"` unless the internal `_step_name` field is set.

## Example

```lua
local flow = Flow.new("if_body_contains_example")

flow:step("seed", nodes.code({
    source = function(ctx)
        return {
            sample = ctx.sample or "Hello from api"
        }
    end
}))

flow:step("route", nodes.if_body_contains({
    source_key = "sample",
    pattern = "api",
    _step_name = "sample_check",
    true_route = "contains_api",
    false_route = "missing_api",
    case_sensitive = false
})):depends_on("seed")

flow:step("contains_api", nodes.log({
    message = "Pattern found in sample: ${ctx.sample}"
})):depends_on("route"):route("contains_api")

flow:step("missing_api", nodes.log({
    message = "Pattern not found"
})):depends_on("route"):route("missing_api")

return flow
```
