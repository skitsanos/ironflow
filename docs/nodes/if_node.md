# `if_node`

Evaluate a condition against the workflow context and select one of two
routes. Dependent steps choose a branch with `:route("<name>")`.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `condition` | string | Yes | — | Condition to evaluate (see [Condition Syntax](#condition-syntax)). |
| `true_route` | string | No | `"true"` | Route selected when the condition is true. |
| `false_route` | string | No | `"false"` | Route selected when the condition is false. |

## Condition Syntax

A condition is exactly one of three forms. `and`, `or`, `not`, `~=` and
parentheses are not supported; compute complex decisions in a `code` step.

| Form | Example | True when |
|------|---------|-----------|
| Comparison | `ctx.user.age >= 18` | the comparison holds |
| Existence | `ctx.token exists` | the key is present, even when it holds `null` |
| Truthiness | `ctx.active` | the value is present and not `false` or `null` |

- **Left operand**: a context path, `ctx.` plus dotted keys (`ctx.user.age`); the `ctx.` prefix is optional. A literal (`true`, `1`) is not a path; a key with such a name is reachable as `ctx.true`. A path that does not resolve is absent.
- **Right operand**: always a literal, never read from the context. Everything after the first operator is the operand, so `ctx.expr != 'a==b'` compares with the text `a==b`.
- **Literals**: numbers (`100`, `-2`, `1.5`); strings in `"..."` or `'...'` (not containing their own quote) or one unquoted word; `true`, `false`; `nil` (absent or `null`).

| Operator | Context value | Result |
|----------|---------------|--------|
| `==` | absent or `null` | equal only to `nil` |
| `==` | boolean, number | equal to the same boolean or number literal |
| `==` | string | equal to an operand with the same text, quoted or not (`"5"` equals `5`) |
| `==` | object or array | `== nil` is false; any other operand fails the step |
| `!=` | any | exact negation of `==` |
| `>` `<` `>=` `<=` | number | numeric comparison with a number literal |
| `>` `<` `>=` `<=` | anything else, including absent and numeric strings | fails the step |

Values of different types are never equal: `ctx.count == '5'` is false for the
number `5`.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `_route_{step_name}` | string | The selected route (`true_route` or `false_route`). |
| `_condition_result_{step_name}` | boolean | Result of the condition. |

`{step_name}` is the name of the step.

## Behavior

- A condition outside the grammar, or an ordering operator with a non-number operand (`ctx.n > nil`, `ctx.a > ctx.b`), is rejected when the flow loads, so `ironflow validate` and `ironflow run` refuse it before any step runs.
- A comparison with no defined result at runtime (ordering a non-number, comparing an object or array with anything but `nil`) fails the step instead of routing `false`. The error names the condition and the value's type, not the value.
- The same conditions are used by [`step_if`](../LUA_FLOW_GUIDE.md#step_if--conditional-step-shorthand).

## Examples

### Route by a numeric comparison

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

### Boolean and nil literals with `step_if`

```lua
flow:step_if("ctx.ready == true", "start", nodes.log({ message = "ready" }))
flow:step_if("ctx.dry_run != true", "write", nodes.log({ message = "writing" }))
flow:step_if("ctx.override == nil", "use_default", nodes.log({ message = "default" }))
```

See `examples/03-control-flow/guard_literals.lua` for a runnable flow.
