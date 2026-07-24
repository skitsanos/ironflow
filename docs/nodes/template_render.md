# `template_render`

Render a string template with context variable interpolation.

## Parameters

| Parameter    | Type   | Required | Default | Description                                                  |
|--------------|--------|----------|---------|--------------------------------------------------------------|
| `template`   | string | Yes      | --      | Template string containing context path placeholders         |
| `output_key` | string | Yes      | --      | Context key under which the rendered result will be stored   |

The `template` parameter supports the shared context path grammar:

- dotted object access: `${ctx.user.email}`
- zero-based array access: `${ctx.items[0].name}`
- JSON double-quoted bracket keys: `${ctx["key.with.dots"]}`

These forms can be combined. Missing paths, out-of-range indexes, and `null`
values resolve to an empty string. Expressions, function calls, and fallback
operators are invalid; compute derived/default values in an explicit workflow
step. Non-`ctx` placeholders such as `${HOME}` remain literal. To render
`${ctx.user}` literally, pass `\${ctx.user}` at runtime (written as
`"\\${ctx.user}"` in a Lua string). Rendering is one pass, so placeholder text
inside a resolved context string is not evaluated again.

## Context Output

- `{output_key}` -- the rendered string, stored under the key specified by the `output_key` parameter

## Example

```lua
local flow = Flow.new("order_confirmation")

flow:step("render", nodes.template_render({
    template = "Hello ${ctx.users[0].name}, your order #${ctx.order_id} is confirmed.",
    output_key = "confirmation_message"
}))

flow:step("done", nodes.log({
    message = "Rendered: ${ctx.confirmation_message}"
})):depends_on("render")

return flow
```

Given a context with `users[0].name = "Alice"` and `order_id = 42`, the output
key `confirmation_message` will contain:

```
Hello Alice, your order #42 is confirmed.
```
