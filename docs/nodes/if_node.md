# `if_node`

Evaluate a condition against the workflow context and set a routing path.

## Parameters

| Parameter     | Type   | Required | Default   | Description                                                                 |
|---------------|--------|----------|-----------|-----------------------------------------------------------------------------|
| `condition`   | string | Yes      | --        | Expression to evaluate (see Condition Syntax below)                         |
| `true_route`  | string | No       | `"true"`  | Route name written to context when the condition evaluates to `true`        |
| `false_route` | string | No       | `"false"` | Route name written to context when the condition evaluates to `false`       |

## Condition Syntax

A condition is exactly one of three forms. There is no `and`, `or`, `not`,
`~=` or parenthesised grouping, and a condition that uses them is rejected
when the flow is loaded; chain guards or compute the decision in a `code`
step when you need more.

| Form | Example | True when |
|------|---------|-----------|
| Comparison | `ctx.user.age >= 18` | the comparison holds (rules below) |
| Existence | `ctx.token exists` | the key is present, even when its value is `null` |
| Truthiness | `ctx.active` | the value is anything except `false`, `null` or a missing key |

### Operands

The left operand is a context path: `ctx.` followed by a key, with dots for
nested objects (`ctx.user.age`). The `ctx.` prefix is optional. A path has
no spaces, quotes, parentheses or operator characters and no empty segment,
and it is not a literal: `true`, `false`, `nil` or a number in place of a
path is rejected, so a condition cannot be the constant `true`. A key with
such a name is still reachable with the prefix (`ctx.true`).
A path that does not resolve is an absent value.

The right operand is always a literal. It is never read from the context, so
`ctx.a > ctx.b` is not a comparison of two values.

The first operator in the condition separates the operands. Anything after
it is the right operand, so a string may contain operator characters:
`ctx.expr != 'a==b'` compares `expr` with the text `a==b`.

| Literal | Written as | Notes |
|---------|------------|-------|
| Number | `100`, `-2`, `1.5` | any decimal number |
| String | `"active"` or `'active'` | may contain spaces and operators, but not its own quote character: write `"it's"`. A single unquoted word such as `active` is also read as a string |
| Boolean | `true`, `false` | |
| Nil | `nil` | stands for a key that is absent or `null` |

### Equality: `==` and `!=`

`!=` is the exact negation of `==` for every pair of operands.

| Context value | Equal to |
|---------------|----------|
| absent or `null` | `nil` only |
| boolean | the same boolean literal |
| number | a number literal with the same value |
| string | an operand with the same text, quoted or not: the string `"5"` equals `5` and `'5'`, and the string `"true"` equals `true` |
| object or array | nothing; `== nil` is false and `!= nil` is true, and any other operand is an error |

Values of different types are not equal, so `ctx.ready == 1` is false for a
boolean and `ctx.count == '5'` is false for a number.

### Ordering: `>`, `<`, `>=`, `<=`

Ordering compares a number in the context with a number literal. Any other
context value is an error, including a missing key and a numeric string such
as `"5"`, which is not converted.

| Context value | Result |
|---------------|--------|
| number | numeric comparison |
| absent, `null`, string, boolean, object or array | error |

A key that may be missing needs a default before it is ordered: set it in an
upstream step or in the run context, or make the decision in a `code` step.

### Errors

A comparison that has no defined result is reported instead of routing
`false`, so a guard cannot skip its step silently.

- A condition outside the grammar is rejected when the flow is loaded:
  `not ctx.ready`, `ctx.a and ctx.b`, `ctx.n == 1 and ctx.s == 'x'`,
  `(ctx.n > 0)`, `ctx.s ~= 'x'`, an unquoted operand of several words
  (`ctx.s == hello world`), an unterminated string, a missing operand, or
  a literal in place of the path (`true`, `1 == 1`).
- An ordering operator whose right operand is not a number (`ctx.n > nil`,
  `ctx.n < 'abc'`, `ctx.a > ctx.b`) is rejected when the flow is loaded, so
  `ironflow validate`, `ironflow run` and the API refuse the flow before any
  step runs. The load error names the step and the condition.
- An ordering operator applied to anything but a number (an absent or `null`
  value, a string, a boolean, an object or an array), or an object or array
  compared with anything other than `nil`, fails the `if_node` step. The
  run finishes `failed`, and the error names the condition and the type of the
  context value, never the value itself. The condition is quoted as written,
  including one on a key with a credential-like name such as `ctx.api_token`.

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

Boolean and nil literals with [`step_if`](../LUA_FLOW_GUIDE.md#step_if--conditional-step-shorthand):

```lua
flow:step_if("ctx.ready == true", "start", nodes.log({ message = "ready" }))
flow:step_if("ctx.dry_run != true", "write", nodes.log({ message = "writing" }))
flow:step_if("ctx.override == nil", "use_default", nodes.log({ message = "default" }))
```

See `examples/03-control-flow/guard_literals.lua` for a runnable flow.

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
