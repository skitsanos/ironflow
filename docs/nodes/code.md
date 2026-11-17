# `code`

Run Lua code in a sandbox with a snapshot of the workflow context and merge its
return value into the context.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string or function | Yes | — | Lua chunk whose last value is the output, or a Lua function called with `ctx`. |
| `context_keys` | array of strings | No | full context | Top-level context keys to expose to Lua; `{}` exposes none. |

A function passed as `source`, or directly as a step handler
(`flow:step("name", function(ctx) ... end)`), is compiled when the flow loads
and can run only in the process that compiled it.

## Context Output

| Return value | Context output |
|--------------|----------------|
| object table | Each key is merged into the context. |
| array table | `result` holds the array. |
| other value | `result` holds the value. |
| `nil` | Nothing is added. |

## Behavior

- Tables must be a dense 1-based array or an object with string keys. Cyclic, sparse, mixed-key, too deep or non-finite values fail the step with an error naming the path.
- Functions must be self-contained: captured locals from the flow file are rejected. Declare constants inside or read them from `ctx`.
- `ironflow validate` compiles string sources and reports undefined globals with line and column; `--strict` makes those warnings failures.
- `context_keys` names exact top-level keys (`"a.b"` is `ctx["a.b"]`). Missing keys are absent. `_error_message`, `_error_step`, `_error_node_type` and `_flow_dir` are always passed; every other key, including `_error_output` and `_headers`, must be listed. Use it to avoid converting large values the code does not read.
- `context_keys` is accepted only on `code` nodes and function handlers (also via the builder, `:context_keys({...})`); on any other node it fails loading.
- `ctx` is a copy: changing it in Lua does not change the workflow context; only the return value is merged.

### Available globals

| Global | Description |
|--------|-------------|
| `ctx` | Snapshot of the workflow context (or the `context_keys` projection). |
| `env(key)` | Environment variable as a string, or `nil`; restricted by `IRONFLOW_ENV_ALLOWLIST` when set. |
| `json_parse(str)` | Parse JSON text to a Lua value; integers outside the 64-bit range come back as digit strings. |
| `json_stringify(value)` | Serialize a Lua value to JSON text. |
| `json_array(t)`, `json_object(t)` | Mark a table, including `{}`, as a JSON array or object. |
| `json_null` | Non-`nil` sentinel for JSON `null`. |
| `base64_encode(s)`, `base64_decode(s)` | Base64 encoding of bytes or text. |
| `log([level], ...)` | Write a log line. |
| `uuid4()` | Random UUID string. |
| `now_rfc3339()`, `now_unix_ms()` | Current UTC time as RFC 3339 text or Unix milliseconds. |

The `table`, `string`, `utf8` and `math` libraries are available. `os`, `io`,
`debug`, `package`, `require`, `load`, `loadfile`, `dofile`, `collectgarbage`
and `string.dump` are not. `Flow` and `nodes` exist only while the flow file
loads.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_LUA_MAX_INSTRUCTIONS` | `5000000` | Lua instructions per execution; `0` disables. |
| `IRONFLOW_LUA_MAX_SECONDS` | `10` | Wall-clock seconds per Lua state; `0` disables. |
| `IRONFLOW_LUA_MAX_MEMORY_BYTES` | `134217728` | Lua memory per state; `0` disables. |
| `IRONFLOW_LUA_MAX_PATTERN_STEPS` | `50000000` | Total steps of `string.find`, `match`, `gmatch` and `gsub` per state; `0` disables. |
| `IRONFLOW_LUA_HOOK_INTERVAL` | `10000` | Instructions between limit, timeout and cancellation checks. |
| `IRONFLOW_LUA_GC_AFTER_EXECUTION` | `true` | Run a garbage-collection cycle after execution. |
| `IRONFLOW_MAX_CONVERSION_NODES` | `100000` | Values converted between JSON and Lua, shared by all selected context keys. |
| `IRONFLOW_MAX_CONVERSION_DEPTH` | `64` | Nesting depth of converted values. |

A step `timeout` and run cancellation also stop running Lua, including inside a
pattern match. Limit errors are ordinary Lua errors that `pcall` can catch, but
a spent pattern budget stays spent for that state. See
[CLI reference](../CLI_REFERENCE.md) for the full variable list.

## Examples

### Compute from context

```lua
local flow = Flow.new("calculate_total")

flow:step("compute", nodes.code({
    source = function()
        local total = ctx.price * ctx.quantity
        return { total = total, currency = ctx.currency or "USD" }
    end
}))

flow:step("done", nodes.log({
    message = "Total: ${ctx.total} ${ctx.currency}"
})):depends_on("compute")

return flow
```

### Read an environment variable

```lua
local flow = Flow.new("env_check")

flow:step("check", nodes.code({
    source = function()
        local api_key = env("API_KEY")
        return { has_key = api_key ~= nil }
    end
}))

flow:step("done", nodes.log({
    message = "API key present: ${ctx.has_key}"
})):depends_on("check")

return flow
```

### Project the context

```lua
flow:step("inspect", nodes.code({
    context_keys = {"embed_count", "embed_dimension"},
    source = function(ctx)
        return { checked_count = ctx.embed_count, dimension = ctx.embed_dimension }
    end
})):depends_on("embed")

flow:step("constant", function(ctx)
    return { ready = true }
end):context_keys({}):depends_on("inspect")
```
