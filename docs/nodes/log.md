# `log`

Write an interpolated message to the workflow log.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `message` | string | No | `""` | Message template; `${ctx.key}` placeholders are replaced from the context. |
| `level` | string | No | `"info"` | `debug`, `info`, `warn` or `error`; any other value logs at `info`. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `log_message` | string | The rendered message. |

## Examples

### Log at two levels

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
