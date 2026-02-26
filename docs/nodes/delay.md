# `delay`

Pause workflow execution for a specified duration.

## Parameters

| Parameter | Type  | Required | Default | Description                         |
|-----------|-------|----------|---------|-------------------------------------|
| `seconds` | float | No       | `1.0`   | Number of seconds to pause (supports fractional values) |

## Context Output

- `delay_seconds` -- the actual number of seconds the node paused for

## Example

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
