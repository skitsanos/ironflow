# `switch_node`

Select a route by matching a context value against a map of cases.
Dependent steps choose a branch with `:route("<name>")`.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `value` | string | Yes | — | Context path to read, such as `ctx.user.role`; the `ctx.` prefix is optional. |
| `cases` | object | Yes | — | Map of case value to route name. |
| `default` | string | No | `"default"` | Route used when no case matches. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `_route_{step_name}` | string | The selected route. |
| `_switch_value_{step_name}` | string | The resolved value as text. |

`{step_name}` is the name of the step.

## Behavior

- The value is compared as text with the case keys: a string as-is, a number or boolean as its JSON text (`5`, `true`), an object or array as compact JSON.
- A path that does not resolve becomes `""`; it does not fail the step.
- A case whose route is not a string is treated as no match.

## Examples

### Route by role

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

flow:step("handle_guest", nodes.log({
    message = "Guest access — limited functionality",
    level = "warn"
})):depends_on("check_role"):route("guest_flow")

return flow
```

If `ctx.user.role` is `"editor"`, `_route_check_role` is `"editor_flow"`.
