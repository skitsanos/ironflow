# `code`

Execute inline Lua code with access to the workflow context.

## Parameters

| Parameter      | Type   | Required | Default | Description                                                         |
|----------------|--------|----------|---------|---------------------------------------------------------------------|
| `source`       | string | No*      | --      | Lua source code to evaluate                                        |
| `bytecode_b64` | string | No*      | --      | Base64-encoded Lua bytecode for function handler mode               |

*Exactly one of `source` or `bytecode_b64` must be provided.

### Source mode

When `source` is provided, the Lua code is evaluated as an expression or chunk. The last expression's value becomes the node output.

### Function handler (bytecode) mode

When `bytecode_b64` is provided, the base64-encoded Lua bytecode is decoded and loaded as a function. The function is called with the `ctx` table as its sole argument, and its return value becomes the node output.

## Sandboxing

The Lua environment is sandboxed. The following modules and globals are removed before execution:

- `os`
- `io`
- `debug`
- `loadfile`
- `dofile`

### Available globals

- `ctx` -- read-only table containing the full workflow context (JSON values are converted to Lua types)
- `env(key)` -- function to read environment variables; returns the value as a string or `nil` if not set

## Return Value Handling

| Return type | Behavior                                                        |
|-------------|-----------------------------------------------------------------|
| Table       | Each key-value pair is merged into the context output           |
| `nil`       | No output is produced                                           |
| Other       | The value is stored under the key `result` in the context output |

## Context Output

- When returning a table: each key in the returned table becomes a context key
- When returning a scalar: `result` -- the returned value
- When returning `nil`: no keys are added

## Example

Inline source:

```lua
local flow = Flow.new("calculate_total")

flow:step("compute", nodes.code({
    source = [[
        local total = ctx.price * ctx.quantity
        return { total = total, currency = ctx.currency or "USD" }
    ]]
}))

flow:step("done", nodes.log({
    message = "Total: ${ctx.total} ${ctx.currency}"
})):depends_on("compute")

return flow
```

Reading an environment variable:

```lua
local flow = Flow.new("env_check")

flow:step("check", nodes.code({
    source = [[
        local api_key = env("API_KEY")
        return { has_key = api_key ~= nil }
    ]]
}))

flow:step("done", nodes.log({
    message = "API key present: ${ctx.has_key}"
})):depends_on("check")

return flow
```

Function handler mode with base64-encoded bytecode:

```lua
local flow = Flow.new("bytecode_demo")

flow:step("run", nodes.code({
    bytecode_b64 = "G0x1YVIAAQQEBAgAGZM..."
}))

return flow
```

The bytecode function receives `ctx` as its argument:

```lua
-- Original source that was compiled to bytecode:
function(ctx)
    return { greeting = "Hello, " .. ctx.name }
end
```
