# `if_body_contains`

Select a route by checking whether a context value contains a substring.
Dependent steps choose a branch with `:route("<name>")`.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key to inspect; dotted paths such as `resp.body` work. |
| `pattern` | string | Yes | — | Plain substring to look for (not a regular expression). |
| `true_route` | string | No | `"true"` | Route when the substring is found. |
| `false_route` | string | No | `"false"` | Route when it is not found. |
| `case_sensitive` | boolean | No | `true` | Match case exactly. |
| `required` | boolean | No | `false` | Fail the step when `source_key` is missing. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `_route_{step_name}` | string | The selected route. |
| `_contains_{step_name}` | boolean | Whether the substring was found. |

`{step_name}` is the name of the step.

## Behavior

- A non-string value is searched as its compact JSON text.
- A missing `source_key` selects `false_route` unless `required` is `true`.
- An empty `pattern` never matches.

## Examples

### Case-insensitive match

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
