# `template_render`

Render a string template with context variable interpolation.

## Parameters

| Parameter    | Type   | Required | Default | Description                                                  |
|--------------|--------|----------|---------|--------------------------------------------------------------|
| `template`   | string | Yes      | --      | Template string containing `${ctx.*}` placeholders           |
| `output_key` | string | Yes      | --      | Context key under which the rendered result will be stored   |

The template engine replaces `${ctx.key}` placeholders with the corresponding values from the workflow context. Dotted paths are supported for nested access (e.g., `${ctx.user.email}`). Missing keys resolve to an empty string.

## Context Output

- `{output_key}` -- the rendered string, stored under the key specified by the `output_key` parameter

## Example

```lua
local flow = Flow.new("order_confirmation")

flow:step("render", nodes.template_render({
    template = "Hello ${ctx.user.name}, your order #${ctx.order_id} is confirmed.",
    output_key = "confirmation_message"
}))

flow:step("done", nodes.log({
    message = "Rendered: ${ctx.confirmation_message}"
})):depends_on("render")

return flow
```

Given a context with `user.name = "Alice"` and `order_id = 42`, the output key `confirmation_message` will contain:

```
Hello Alice, your order #42 is confirmed.
```
