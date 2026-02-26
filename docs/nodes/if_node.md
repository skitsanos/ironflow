# `if_node`

Evaluate a condition against the workflow context and set a routing path.

## Parameters

| Parameter     | Type   | Required | Default   | Description                                                                 |
|---------------|--------|----------|-----------|-----------------------------------------------------------------------------|
| `condition`   | string | Yes      | --        | Expression to evaluate (see Condition Syntax below)                         |
| `true_route`  | string | No       | `"true"`  | Route name written to context when the condition evaluates to `true`        |
| `false_route` | string | No       | `"false"` | Route name written to context when the condition evaluates to `false`       |

## Condition Syntax

Conditions reference context values with the `ctx.` prefix and support dotted paths for nested access (e.g., `ctx.user.age`).

### Comparison operators

| Operator | Numeric | String | Description              |
|----------|---------|--------|--------------------------|
| `==`     | Yes     | Yes    | Equal                    |
| `!=`     | Yes     | Yes    | Not equal                |
| `>`      | Yes     | No     | Greater than             |
| `<`      | Yes     | No     | Less than                |
| `>=`     | Yes     | No     | Greater than or equal to |
| `<=`     | Yes     | No     | Less than or equal to    |

### Existence check

```
ctx.some_key exists
```

Returns `true` when the key is present in the context (including if the value is `null`).

### Bare truthy check

```
ctx.some_key
```

Returns `true` for any non-null, non-false value. Returns `false` for missing keys, `null`, and `false`.

## Context Output

- `_route_{step_name}` -- the selected route string (`true_route` or `false_route`)
- `_condition_result_{step_name}` -- boolean result of the condition evaluation

`{step_name}` defaults to `"if"` unless the internal `_step_name` field is set.

## Example

```lua
local flow = Flow.new("age_check")

flow:step("check_age", nodes.if_node({
    condition = "ctx.user.age >= 18",
    true_route = "adult",
    false_route = "minor"
}))

flow:step("adult_path", nodes.log({
    message = "Welcome, ${ctx.user.name} — you have full access",
    level = "info"
})):depends_on("check_age"):route("adult")

flow:step("minor_path", nodes.log({
    message = "Sorry, ${ctx.user.name} — restricted access",
    level = "warn"
})):depends_on("check_age"):route("minor")

return flow
```

String comparison example:

```lua
local flow = Flow.new("status_gate")

flow:step("check_status", nodes.if_node({
    condition = 'ctx.status == "active"',
    true_route = "proceed",
    false_route = "skip"
}))

flow:step("process", nodes.log({
    message = "Account is active — proceeding"
})):depends_on("check_status"):route("proceed")

return flow
```
