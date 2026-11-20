# `if_http_status`

Select a route from an HTTP status code held in the context. Dependent steps
choose a branch with `:route("<name>")`.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `status_key` | string | Yes | — | Context key holding the status; dotted paths such as `resp.status` work. |
| `success_route` | string | No | `"success"` | Route for 2xx statuses when `routes` is not set. |
| `error_route` | string | No | `"error"` | Route for other statuses when `routes` is not set. |
| `routes` | object | No | — | Map of status rules to route names (see Behavior). |
| `default_route` | string | No | value of `error_route` | Route when `routes` is set and no rule matches. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `_route_{step_name}` | string | The selected route. |
| `_status_code_{step_name}` | number | The status code. |
| `_status_class_{step_name}` | string | The status class, such as `"4xx"`. |

`{step_name}` is the name of the step.

## Behavior

- With `routes`, the lookup order is the exact code (`"404"`), then the class (`"4xx"`), then a `default` entry in `routes`, then `default_route`.
- The status may be a number or a numeric string; a missing key or any other value fails the step.

## Examples

### Route by status code and class

```lua
local flow = Flow.new("if_http_status_example")

flow:step("seed", nodes.code({
    source = function(ctx)
        return {
            target_status = ctx.target_status or "401"
        }
    end
}))

flow:step("probe", nodes.http_get({
    url = "https://httpbin.org/status/${ctx.target_status}",
    output_key = "probe"
})):depends_on("seed")

flow:step("route", nodes.if_http_status({
    status_key = "probe_status",
    default_route = "unexpected",
    routes = {
        ["2xx"] = "success",
        ["401"] = "unauthorized",
        ["429"] = "rate_limited"
    }
})):depends_on("probe")

flow:step("success", nodes.log({
    message = "HTTP ${ctx._status_code_route} ok",
    level = "info"
})):depends_on("route"):route("success")

flow:step("unauthorized", nodes.log({
    message = "HTTP ${ctx._status_code_route} requires authentication",
    level = "warn"
})):depends_on("route"):route("unauthorized")

flow:step("unexpected", nodes.log({
    message = "Unhandled status class ${ctx._status_class_route}: ${ctx._status_code_route}",
    level = "error"
})):depends_on("route"):route("unexpected")

return flow
```
