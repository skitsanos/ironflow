# `switch_node`

Multi-case routing based on a context value, similar to a switch/case statement.

## Parameters

| Parameter | Type   | Required | Default     | Description                                                        |
|-----------|--------|----------|-------------|--------------------------------------------------------------------|
| `value`   | string | Yes      | --          | Expression that resolves to a context value (e.g., `ctx.role`)     |
| `cases`   | object | Yes      | --          | Map of case values to route names                                  |
| `default` | string | No       | `"default"` | Route name used when no case matches the resolved value            |

The `value` expression is resolved from the context using the `ctx.` prefix and supports dotted paths. The resolved value is compared as a string against each key in `cases`. If a match is found, the corresponding route is used; otherwise `default` is used.

## Context Output

- `_route_{step_name}` -- the selected route string
- `_switch_value_{step_name}` -- the resolved value that was matched against cases

`{step_name}` defaults to `"switch"` unless the internal `_step_name` field is set.

## Example

```lua
local flow = Flow.new("role_routing")

flow:step("check_role", nodes.switch_node({
    value = "ctx.user.role",
    cases = {
        admin = "admin_flow",
        editor = "editor_flow",
        viewer = "viewer_flow"
    },
    default = "guest_flow"
}))

flow:step("handle_admin", nodes.log({
    message = "Admin access granted for ${ctx.user.name}",
    level = "info"
})):depends_on("check_role"):route("admin_flow")

flow:step("handle_editor", nodes.log({
    message = "Editor dashboard loaded for ${ctx.user.name}",
    level = "info"
})):depends_on("check_role"):route("editor_flow")

flow:step("handle_viewer", nodes.log({
    message = "Read-only view for ${ctx.user.name}",
    level = "info"
})):depends_on("check_role"):route("viewer_flow")

flow:step("handle_guest", nodes.log({
    message = "Guest access — limited functionality",
    level = "warn"
})):depends_on("check_role"):route("guest_flow")

return flow
```

If `ctx.user.role` is `"editor"`, the route `_route_switch` will be set to `"editor_flow"`.
