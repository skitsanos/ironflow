# `delay`

Pause workflow execution for a specified duration.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `seconds` | number | No | `1.0` | Seconds to pause; fractional values work. Must be non-negative and finite. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `delay_seconds` | number | The configured pause, in seconds. |

## Examples

### Pause between steps

```lua
local flow = Flow.new("rate_limited")

flow:step("fetch_data", nodes.log({
    message = "Fetching data from API..."
}))

flow:step("wait", nodes.delay({
    seconds = 2.5
})):depends_on("fetch_data")

flow:step("done", nodes.log({
    message = "Resumed after ${ctx.delay_seconds}s pause"
})):depends_on("wait")

return flow
```
