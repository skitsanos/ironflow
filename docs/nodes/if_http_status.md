# `if_http_status`

Route execution based on an HTTP status value in context.

## Parameters

| Parameter | Type   | Required | Default | Description |
|-----------|--------|----------|---------|-------------|
| `status_key` | string | Yes | -- | Context key containing the HTTP status (supports dotted paths) |
| `success_route` | string | No | `success` | Route name used for 2xx statuses when `routes` is not set |
| `error_route` | string | No | `error` | Route name used for non-2xx statuses when `routes` is not set |
| `default_route` | string | No | value of `error_route` | Fallback route when `routes` is set and no match is found |
| `routes` | object | No | -- | Optional map of status rules to route names |

The optional `routes` object allows explicit routing by status code and class.

- Exact status match first (for example `"404" = "not_found"`)
- Then status class match (`"2xx"`, `"4xx"`, etc.)
- Otherwise `default` in `routes`, or `default_route`

## Context Output

- `_route_{step_name}` -- selected route name
- `_status_code_{step_name}` -- numeric status code
- `_status_class_{step_name}` -- class bucket, e.g. `2xx`

## Example

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
    _step_name = "probe",
    default_route = "unexpected",
    routes = {
        ["2xx"] = "success",
        ["401"] = "unauthorized",
        ["429"] = "rate_limited"
    }
})):depends_on("probe")

flow:step("success", nodes.log({
    message = "HTTP ${ctx._status_code_probe} ok",
    level = "info"
})):depends_on("route"):route("success")

flow:step("unauthorized", nodes.log({
    message = "HTTP ${ctx._status_code_probe} requires authentication",
    level = "warn"
})):depends_on("route"):route("unauthorized")

flow:step("rate_limited", nodes.log({
    message = "Rate limit hit (status ${ctx._status_code_probe})",
    level = "warn"
})):depends_on("route"):route("rate_limited")

flow:step("unexpected", nodes.log({
    message = "Unhandled status class ${ctx._status_class_probe}: ${ctx._status_code_probe}",
    level = "error"
})):depends_on("route"):route("unexpected")

return flow
