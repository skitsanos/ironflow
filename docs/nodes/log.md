# `log`

Write a message to the workflow log via the `tracing` framework.

## Parameters

| Parameter | Type   | Required | Default  | Description                                                   |
|-----------|--------|----------|----------|---------------------------------------------------------------|
| `message` | string | No       | `""`     | Message template with `${ctx.*}` interpolation support        |
| `level`   | string | No       | `"info"` | Log level: `debug`, `info`, `warn`, or `error`                |

The message string supports `${ctx.key}` placeholders which are replaced with values from the workflow context before logging. Any unrecognized `level` value falls back to `info`.

## Context Output

- `log_message` -- the rendered (interpolated) message that was logged

## Example

```lua
local flow = Flow.new("order_logging")

flow:step("info", nodes.log({
    message = "Processing order ${ctx.order_id} for user ${ctx.user.name}",
    level = "info"
}))

flow:step("warn", nodes.log({
    message = "Order ${ctx.order_id} exceeds $1000 — flagged for review",
    level = "warn"
})):depends_on("info")

return flow
```
