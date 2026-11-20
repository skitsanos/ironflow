# `template_render`

Render a string template by replacing `${ctx...}` placeholders with context values.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `template` | string | Yes | — | Text containing `${ctx...}` placeholders |
| `output_key` | string | Yes | — | Context key for the rendered string |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | The rendered text |

## Behavior

- Placeholders use the context path grammar: dotted access (`${ctx.user.email}`), zero-based indexes (`${ctx.items[0].name}`), and JSON double-quoted keys (`${ctx["key.with.dots"]}`), in any combination.
- A missing path, an out-of-range index, or `null` renders as an empty string. Objects and arrays render as compact JSON.
- Expressions, function calls and fallback operators are not supported and are rejected when the flow is validated; compute derived or default values in an earlier step.
- Non-`ctx` placeholders such as `${HOME}` stay literal.
- `\${ctx.user}` renders literally as `${ctx.user}` (write `"\\${ctx.user}"` in a Lua string).
- Rendering is one pass: placeholder text inside a resolved value is not evaluated again.

## Examples

### Render a message

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

With `users[0].name = "Alice"` and `order_id = 42`, `confirmation_message` is `Hello Alice, your order #42 is confirmed.`
