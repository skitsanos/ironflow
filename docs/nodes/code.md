# `code`

Execute inline Lua code with access to the workflow context.

## Parameters

| Parameter      | Type   | Required | Default | Description                                                         |
|----------------|--------|----------|---------|---------------------------------------------------------------------|
| `source`       | string/function | No* | --      | Lua source code string **or inline function** to evaluate              |
| `bytecode_b64` | string | No*      | --      | Base64-encoded Lua bytecode for function handler mode               |
| `context_keys` | array of strings | No | Full context | Literal top-level user context keys to expose to Lua; an empty list exposes none. Engine-reserved `_` keys always pass through |

*Exactly one of `source` or `bytecode_b64` must be provided.

### Source mode

When `source` is provided, one of two forms are supported:

- Lua source string, evaluated as an expression/chunk. The last expression's value becomes the node output.
- Lua function, which is compiled to bytecode and executed with sandboxed context as the script body.

Validation compiles string-valued source without executing it and rejects
invalid syntax. Undefined global reads produce warnings with the step name and
line/column positions relative to the decoded source string. Use
`ironflow validate --strict` to reject those warnings.

### Function handler (bytecode) mode

When `bytecode_b64` is provided, the base64-encoded Lua bytecode is decoded and loaded as a function. The function is called with the `ctx` table as its sole argument, and its return value becomes the node output.

Inline function handlers must not capture locals from the enclosing flow file;
IronFlow rejects such handlers because captured upvalues cannot survive
serialization. Declare constants inside the handler or pass values through
`ctx`. During validation, reads of undefined globals inside inline functions
produce source-positioned warnings; `ironflow validate --strict` treats them as
failures.

The same self-contained function may be used by multiple code nodes or step
handlers. Validation reports its source warnings once. Distinct function
definitions must have distinguishable start/end line ranges; move same-range
definitions onto separate lines if validation reports ambiguity. See
[`reused_callbacks.lua`](../../examples/07-advanced/reused_callbacks.lua).

### Context projection

By default, the entire accumulated context is converted before Lua runs, even
if the code reads only one key. Opt in to `context_keys` when earlier steps
produce large values that this handler does not need:

```lua
flow:step("inspect", nodes.code({
    context_keys = {"embed_count", "embed_dimension"},
    source = function(ctx)
        return { checked_count = ctx.embed_count, dimension = ctx.embed_dimension }
    end
})):depends_on("embed")
```

The shorthand function-handler API supports the same projection through its
chainable builder, including handlers created with `flow:step_if`:

```lua
flow:step("inspect", function(ctx)
    return { checked_count = ctx.embed_count }
end):context_keys({"embed_count"}):depends_on("embed")

flow:step("constant", function(ctx)
    return { ready = true }
end):context_keys({}):depends_on("inspect")
```

- Omit `context_keys` to preserve the full-context default. Lua `{}` (JSON `[]`)
  exposes no user keys, not the default context.
- Engine-reserved keys always pass through regardless of the list: any key
  starting with `_`, including the recovery overlay (`_error_message`,
  `_error_step`, `_error_node_type`, `_error_output`) and `_flow_dir`. The list
  governs user data keys only, so a projected `on_error` handler keeps its
  diagnostics; listing a `_` key explicitly is harmless. Projection is a
  conversion-budget control, not a secrecy boundary.
- Keys are exact, case-sensitive top-level names, not paths or templates.
  `"a.b"` selects `ctx["a.b"]`, not `ctx.a.b`. Missing keys remain absent (`nil`);
  duplicate names are selected once. Invalid lists fail loading/validation or
  direct node execution rather than silently exposing the full context.
- A builder's projection belongs only to that step, even when multiple steps
  reuse the same `nodes.code` descriptor.
- Both the global `ctx` and the bytecode function's argument refer to the same
  isolated snapshot. Projection does not remove data from the workflow context,
  and Lua mutations do not modify stored input values. Outputs merge normally.
- Conversion remains eager for selected values. The object root and **all**
  selected keys share one `IRONFLOW_MAX_CONVERSION_NODES` budget (default
  `100000`) and the original `IRONFLOW_MAX_CONVERSION_DEPTH` limit (default `64`).
  There is no per-key reset. Selecting a 200,000-number array still fails before
  the handler runs, even if it would read only its first element or nothing.
- Projection does not bypass Lua memory/instruction/time limits or output
  conversion limits, change step dependencies, or project a `step_if` guard.
  It is available on code nodes and function handlers only. Declaring
  `context_keys` on any other node type, whether through the builder or a
  descriptor such as `nodes.log({ context_keys = {...} })` or
  `nodes.foreach({ context_keys = {...} })`, is rejected at load time with
  `Step '<name>': context_keys is only supported for code nodes and function
  handlers`, so `ironflow validate` fails instead of silently running with the
  full context.

See the self-contained
[`context_projection.lua`](../../examples/07-advanced/context_projection.lua)
example, which retains a 200,000-number native-node output while subsequent
handlers inspect only a scalar or use an empty snapshot.

## Sandboxing

The Lua VM starts from an allowlist containing computation-oriented table,
string, UTF-8, and math functionality. In particular, the following capabilities
are unavailable:

- `os`
- `io`
- `debug`
- `package`
- `require`
- `load`
- `loadfile`
- `dofile`
- `collectgarbage`
- `string.dump`

### Available globals

- `ctx` -- isolated table snapshot of the workflow context, optionally projected with `context_keys` (JSON values are converted to Lua types)
- `env(key)` -- function to read environment variables; returns the value as a string or `nil` if not set
- `json_parse(str)` -- parse JSON text into a Lua table
- `json_stringify(value)` -- serialize a Lua value to JSON text
- `json_array(table)` -- mark a dense Lua table, including `{}`, as a JSON array
- `json_object(table)` -- mark a string-keyed Lua table, including `{}`, as a JSON object
- `json_null` -- non-`nil` sentinel that preserves JSON null fields and array entries
- `base64_encode(str)` -- encode bytes/text to base64
- `base64_decode(str)` -- decode base64 to bytes/text
- `log([level], message...)` -- write a Lua log line
- `uuid4()` -- generate a random UUID string
- `now_rfc3339()` -- current UTC timestamp in RFC3339 format
- `now_unix_ms()` -- current Unix timestamp in milliseconds

`Flow` and `nodes` exist only while the flow definition is loaded and are not
available in the isolated handler VM.

### Execution limits

Lua execution runs on Tokio's blocking pool and uses process-wide budgets:

- `IRONFLOW_LUA_MAX_INSTRUCTIONS` — default `5000000`; `0` disables.
- `IRONFLOW_LUA_MAX_SECONDS` — default `10`; `0` disables.
- `IRONFLOW_LUA_MAX_MEMORY_BYTES` — default `134217728`; `0` disables.
- `IRONFLOW_LUA_HOOK_INTERVAL` — default `10000`.
- `IRONFLOW_LUA_GC_AFTER_EXECUTION` — default `true`.

When the flow step also uses `:timeout(seconds)`, the Lua instruction hook
checks that total step deadline in addition to these limits. Dropping the node
future (for example, explicit workflow cancellation) signals the same hook, so
an infinite loop is interrupted without occupying a Tokio runtime worker. Hook
checks occur every `IRONFLOW_LUA_HOOK_INTERVAL` instructions, so cancellation
latency is cooperative rather than instruction-exact.

## Return Value Handling

| Return type | Behavior                                                        |
|-------------|-----------------------------------------------------------------|
| Object table | Each key-value pair is merged into the context output          |
| Array table | The array is stored under `result`                              |
| `nil`       | No output is produced                                           |
| Other       | The value is stored under the key `result` in the context output |

Tables must have one JSON-compatible shape: a dense 1-based array or an object
with string keys. Cyclic, over-deep, sparse, mixed-key, non-finite, and
unsupported values fail the node with a path-rich error instead of being
silently discarded.

## Context Output

- When returning an object table: each key becomes a context key
- When returning an array table: `result` -- the returned array
- When returning a scalar: `result` -- the returned value
- When returning `nil`: no keys are added

## Example

Inline source:

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

Reading an environment variable:

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
