# Writing Flows in Lua

## Overview

IronFlow flows are defined in Lua scripts. Each `.lua` file describes a workflow — its steps, dependencies between them, and configuration like retries and timeouts.

The Rust engine loads the Lua file, extracts the flow definition, builds a DAG, and executes it.

## Basic Structure

Every flow file must return a `Flow` object:

```lua
local flow = Flow.new("my_flow_name")

-- define steps
flow:step("step_name", nodes.node_type({ ... config ... }))

return flow
```

## Defining Steps

```lua
flow:step("fetch_data", nodes.http_get({
    url = "https://api.example.com/data",
    headers = { Authorization = "Bearer ${ctx.token}" },
    output_key = "api"
}))
```

Each step has:
- **name** (string) — unique identifier within the flow (duplicates are rejected)
- **handler** — either a node config (`nodes.node_type({...})`) or a Lua function (`function(ctx) ... end`)

## Dependencies

Steps run in parallel by default. Use `depends_on()` to enforce ordering:

```lua
flow:step("process", nodes.data_transform({
    source_key = "api_data",
    mapping = { name = "full_name", age = "years" },
    output_key = "processed"
})):depends_on("fetch_data")
```

Independent steps receive the same phase-start context snapshot and cannot
read one another's output. A dependency is therefore required for data flow,
even when a concurrency limit would happen to run those steps one at a time.

Multiple dependencies:

```lua
flow:step("merge", nodes.log({
    message = "Both sources ready: ${ctx.users_count} users, ${ctx.orders_count} orders"
})):depends_on("fetch_users", "fetch_orders")
```

## Retries

Configure retry behavior per step:

```lua
flow:step("call_api", nodes.http_post({
    url = "https://api.example.com/submit",
    body = { data = "payload" },
    output_key = "result"
})):retries(3, 1.0)  -- max 3 retries, 1s initial backoff
```

The backoff is exponential: 1s → 2s → 4s.

## Timeout

Set a per-step timeout:

```lua
flow:step("slow_op", nodes.shell_command({
    cmd = "long-running-script.sh",
    timeout = 30
})):retries(3, 1.0):timeout(30)  -- one 30 second total budget
```

`timeout()` is a **total step execution budget**, not a per-attempt timeout. It
starts before the first attempt and is shared by every node attempt and each
exponential retry backoff. A retry starts only if its backoff completes before
that deadline; the timeout error is persisted when the budget expires.
Node-local timeouts such as `shell_command.timeout` and HTTP `timeout` remain
independent upper bounds, but the shorter enclosing step budget wins.

Cancellation is cooperative and resource-specific:

- async node work is cancelled by dropping its future;
- `code`, `foreach`, and nested-flow parsing run away from Tokio worker threads
  and check the step deadline/cancellation through the Lua instruction hook;
- structured child workflows are cancelled with their parent step;
- shell children and invalidated MCP stdio sessions terminate their direct
  child on every platform, and kill the inherited process group on Unix;
- fire-and-forget `subworkflow(wait = false)` work is intentionally detached;
- an external request or other side effect that completed before cancellation
  cannot be rolled back. Custom synchronous nodes must use cooperative
  checkpoints; an uncooperative blocking call may finish after its waiter has
  timed out.

State/event persistence needed to record the terminal task outcome is not
forcibly interrupted by the execution deadline, so backend latency can make
the observed workflow completion later than the configured timeout.

## Error Recovery

Use `on_error()` to associate one failing step with one dedicated recovery
step. Recovery is part of the execution plan: the handler runs after the source
step fails and after all of the handler's declared dependencies complete.
Retries and `timeout()` on the handler work exactly as they do for normal steps.

```lua
flow:step("prepare_recovery", nodes.code({
    source = "return { recovery_channel = 'operations' }"
}))

flow:step("risky", nodes.read_file({
    path = "/path/that/may-not-exist"
})):on_error("recover")

flow:step("recover", nodes.code({
    source = [[
        return {
            recovered_step = ctx._error_step,
            recovery_message = ctx._error_message,
            recovery_channel = ctx.recovery_channel
        }
    ]]
})):depends_on("prepare_recovery")

-- This ordinary downstream step waits until recovery has resolved `risky`.
flow:step("continue", nodes.log({
    message = "Recovery result: ${ctx.recovered_step}"
})):depends_on("risky")
```

The handler receives three invocation-local context values for every failure:

- `_error_message` — the source step's final error after retries;
- `_error_step` — the source step name;
- `_error_node_type` — the source node type.

When a node returns a structured failure, the handler also receives
`_error_output`: an object containing the final attempt's exact, redacted
output. Those fields are also published under their normal context keys after
retries are exhausted and the source phase settles. `_error_output` remains
useful when parallel tasks use the same output-key prefix because it preserves
the source task's exact output even if another task wins the shared-context
collision. Ordinary errors and execution timeouts do not have this object. For
example, a failed `shell_command` can be inspected as
`ctx._error_output.shell_stderr`; see
[`shell_command`](nodes/shell_command.md) for its exit-policy contract.

The `_error_*` keys do not enter shared workflow context automatically. A
handler can persist selected information by returning it in its normal node
output.
Distinct handlers can therefore execute concurrently without overwriting one
another's error metadata.

A successful handler resolves the source failure for scheduling and final run
status. The source task remains `Failed` in durable history for auditability,
while the handler is `Success`. If the handler fails or cannot run because one
of its own dependencies failed, the source failure remains unresolved and the
run finishes `Failed`. When the source does not fail, its handler is `Skipped`;
ordinary source dependents continue, while a recovery-only branch that
explicitly depends on the handler is skipped.

Recovery handlers are intentionally dedicated steps. Validation rejects a
missing or self-referencing target, a handler shared by multiple sources, a
handler with its own `on_error()` or `route()`, a handler that depends directly
on its failed source, and cycles in the combined normal/recovery graph. A
handler can otherwise use normal dependencies, retries, and a timeout.

## Context

Context is a shared key-value store that flows through all steps:

```lua
-- Initial context is passed via CLI or API:
-- ironflow run flow.lua --context '{"user_id": "123"}'

-- Steps read from context via ${ctx.key} interpolation:
flow:step("greet", nodes.template_render({
    template = "Hello, ${ctx.user_name}!",
    output_key = "greeting"
}))

-- Step outputs are committed back into context at the phase barrier.
-- After "greet" runs, ctx.greeting = "Hello, Alice!"
```

Each DAG phase reads one immutable phase-start context snapshot. Independent
steps cannot observe peer outputs, regardless of completion timing, retries,
or the task-concurrency limit. After the phase settles, successful outputs and
terminal structured-failure outputs are committed in flow declaration order.
If more than one step publishes the same key, the later-declared step wins. A
later dependency phase likewise overwrites an existing key. Skipped steps and
failures without structured output publish nothing.

Per-task history remains task-local, so collision precedence never substitutes
another task's value; it is still subject to `IRONFLOW_MAX_TASK_OUTPUT_BYTES`,
which replaces an oversized record with a truncation marker. Lifecycle-event,
log, and external-side-effect completion order remains timing-dependent and
does not define context precedence. Add `depends_on()` when one step must read
another; use distinct `output_key` prefixes or namespaced keys when parallel
values must coexist. If a phase is cancelled or stalls before its barrier,
none of that phase's buffered output enters final run context, although
already-terminal bounded task history remains available.

### Context variable interpolation

Node parameter documentation identifies the string fields that resolve context
placeholders at runtime. Interpolation is opt-in per field; IronFlow does not
rewrite every string in a node config.

Supported fields accept context paths:

```lua
nodes.http_get({
    url = "https://api.example.com/users/${ctx.user_id}"
})

nodes.template_render({
    template = 'Email: ${ctx.user.email}; first item: ${ctx.items[0].name}; label: ${ctx["key.with.dots"]}',
    output_key = "info"
})
```

The path grammar supports:

- dot-separated object keys, as in `${ctx.user.email}`
- zero-based array indexes, as in `${ctx.items[0].name}` (even though ordinary
  Lua table indexes start at one)
- JSON double-quoted bracket keys, as in `${ctx["key.with.dots"]}`

These forms can be combined. Interpolation performs lookup only: arithmetic,
boolean operators, function calls, and fallback expressions are invalid. Use
an explicit function handler or `code` step to compute a default before a
dependent node interpolates it.

Missing paths, out-of-range indexes, and `null` values render as an empty
string. Placeholders outside the reserved `ctx` namespace, such as `${HOME}`
or `${TMPDIR:-/tmp}`, remain unchanged. To emit `${ctx.user}` literally, the
runtime text must contain `\${ctx.user}`; write that as `"\\${ctx.user}"` in a
Lua string. Resolution is one pass: a placeholder contained inside a resolved
context string is not evaluated again. Numbers, booleans, arrays, and objects
render as compact JSON.

## Environment Variables

Use `env(key)` to read the final merged process environment in Lua. Existing
shell, container, or service variables take precedence over values in the
selected dotenv file. Dotenv is loaded atomically before the flow is parsed;
see the [CLI configuration contract](CLI_REFERENCE.md#configuration-resolution)
for discovery, error, and precedence rules.

By default `env()` can read any process variable. Set `IRONFLOW_ENV_ALLOWLIST`
to a comma-separated list of variable names to restrict it: when set, `env()`
returns `nil` for any name not on the list, so flows cannot read (or exfiltrate)
credentials the deployment did not explicitly expose.

```lua
local api_key = env("API_KEY")
local db_url = env("DATABASE_URL") or "sqlite://default.db"

flow:step("call_api", nodes.http_get({
    url = "https://api.example.com/data",
    auth = { type = "bearer", token = env("API_TOKEN") },
    output_key = "api"
}))
```

## Webhook Context

When a flow is triggered via `POST /webhooks/{name}`, the engine injects:

- `ctx._headers` — a table containing only the lowercase header names listed
  in that webhook's `forward_headers` configuration
- `ctx._webhook` — the webhook name from the URL

```yaml
webhooks:
  signed-action:
    flow: signed_action.lua
    forward_headers:
      - x-webhook-signature
```

```lua
flow:step("check_auth", function(ctx)
    local signature = ctx._headers and ctx._headers["x-webhook-signature"] or ""
    local expected = env("WEBHOOK_SHARED_SECRET")
    if not expected or signature ~= expected then
        error("Unauthorized")
    end
    return { signature_valid = true }
end)
```

No request headers are forwarded by default. Platform authentication headers
such as `Authorization` and `X-API-Key`, cookies, and proxy/session credentials
are never workflow input. Configured header values are execution-only and are
redacted from literal outputs, errors, child runs, stored context, events, and
run-detail responses. Validate them in place; do not log, return, transform, or
send their raw values elsewhere. The engine cannot recognize every derived or
deliberately exfiltrated form of a secret.

`ctx._webhook` is durable workflow metadata. `ctx._headers` exists only during
webhook execution and is inherited by nested workflow execution without being
persisted. Flows run via CLI or `/flows/run` do not receive it.

## Conditional Execution

Use conditional nodes to branch:

```lua
flow:step("check", nodes.if_node({
    condition = "ctx.amount > 100",
    true_route = "high_value",
    false_route = "normal"
}))

flow:step("high_value_handler", nodes.log({
    message = "VIP order: $${ctx.amount}"
})):depends_on("check"):route("high_value")

flow:step("normal_handler", nodes.log({
    message = "Standard processing for $${ctx.amount}"
})):depends_on("check"):route("normal")
```

### `step_if` — Conditional Step Shorthand

For the common case of "run this step only if a condition is true", use `step_if` instead of manually wiring an `if_node` + `depends_on` + `route`:

```lua
-- Instead of:
flow:step("check", nodes.if_node({ condition = "ctx.score > 50" }))
flow:step("bonus", nodes.code({ source = "return { got_bonus = true }" })):depends_on("check"):route("true")

-- Write:
flow:step_if("ctx.score > 50", "bonus", nodes.code({ source = "return { got_bonus = true }" }))
```

`step_if` accepts the same condition syntax as `if_node` (`ctx.key > N`, `ctx.key == "value"`, `ctx.key exists`, bare truthiness). It creates an auto-named guard step (`_if_<step_name>`) and wires the actual step to run only when the condition is true. If false, the step is skipped.

Function handlers work too:

```lua
flow:step_if("ctx.active", "greet", function(ctx)
    return { greeting = "Hello " .. ctx.name }
end)
```

The returned builder supports the same chainable methods: `depends_on()`, `retries()`, `timeout()`, `on_error()`.

Multi-case routing with `switch_node`:

```lua
flow:step("route", nodes.switch_node({
    value = "ctx.tier",
    cases = { free = "free_path", pro = "pro_path" },
    default = "free_path"
}))

flow:step("handle_free", nodes.log({
    message = "Free tier"
})):depends_on("route"):route("free_path")

flow:step("handle_pro", nodes.log({
    message = "Pro tier"
})):depends_on("route"):route("pro_path")
```

## Available Nodes

See [NODE_REFERENCE.md](NODE_REFERENCE.md) for the complete list of 100 built-in nodes and their configuration options.

## Inline Lua Code

Use the `code` node to run custom Lua logic with full access to the workflow context:

```lua
-- Extract fields from an API response
flow:step("extract", nodes.code({
    source = [[
        local data = ctx.api_data
        return {
            user_name = data.user.name,
            user_email = data.user.email
        }
    ]]
})):depends_on("call_api")
```

The code runs in a sandboxed Lua VM without OS, I/O, package-loading, or dynamic
code-loading access. Return a string-keyed object table to publish its fields at
the current phase barrier, or return a scalar/array (stored under `result`).
Returned data must be JSON-compatible: cyclic, sparse, mixed-key, and
unsupported values are rejected. Use `json_array({})`, `json_object({})`, and
`json_null` when an empty table or null value would otherwise be ambiguous.

## Function Handlers

You can pass a Lua function directly as a step handler — no need for `nodes.code()`:

```lua
flow:step("process", function(ctx)
    local admins = {}
    for _, user in ipairs(ctx.users) do
        if user.role == "admin" then
            table.insert(admins, string.upper(user.name))
        end
    end
    return { admins = admins, admin_count = #admins }
end)
```

The function receives `ctx` (the phase-start workflow context) as its argument
and returns a table of key-value pairs to publish at the phase barrier. Under
the hood, the function is compiled to bytecode at parse time and executed as a
`code` node — so the same sandbox rules apply. `env()` works inside handlers.

**Important:** Function handlers must be self-contained. Do not capture local variables from the enclosing scope — they will be `nil` at runtime:

```lua
-- BAD: captured local won't survive bytecode transfer
local threshold = 100
flow:step("check", function(ctx)
    return { over = ctx.amount > threshold }  -- threshold is nil!
end)

-- GOOD: use env() or inline the value
flow:step("check", function(ctx)
    return { over = ctx.amount > 100 }
end)
```

## Complete Example

```lua
-- order_processing.lua
local flow = Flow.new("process_order")

-- Validate the incoming order
flow:step("validate", nodes.validate_schema({
    source_key = "order",
    schema = {
        type = "object",
        required = { "order_id", "customer_name", "items", "total" },
        properties = {
            order_id = { type = "string" },
            customer_name = { type = "string" },
            items = { type = "array" },
            total = { type = "number" }
        }
    }
}))

-- Compute a checksum for the order
flow:step("checksum", nodes.hash({
    source_key = "order",
    algorithm = "sha256",
    output_key = "order_hash"
})):depends_on("validate")

-- Check if it's a high-value order
flow:step("check_value", nodes.if_node({
    condition = "ctx.order.total > 500",
    true_route = "high_value",
    false_route = "standard"
})):depends_on("validate")

-- High-value path: log for review
flow:step("flag_review", nodes.log({
    message = "HIGH VALUE ORDER ${ctx.order.order_id}: $${ctx.order.total} from ${ctx.order.customer_name} (hash: ${ctx.order_hash})",
    level = "warn"
})):depends_on("check_value", "checksum"):route("high_value")

-- Standard path: log confirmation
flow:step("confirm", nodes.log({
    message = "Order ${ctx.order.order_id} confirmed: $${ctx.order.total} for ${ctx.order.customer_name}",
    level = "info"
})):depends_on("check_value", "checksum"):route("standard")

return flow
```

Run with:
```bash
ironflow run order_processing.lua \
  --context '{"order":{"order_id":"ORD-42","customer_name":"Alice","items":["widget"],"total":750}}'
```

## Tips

- Keep flows focused — one flow per logical workflow
- Use meaningful step names — they appear in logs and state
- Step names must be unique within a flow (duplicates cause a parse error)
- Set retries on external calls (HTTP, shell, etc.)
- Set timeouts on potentially slow operations
- Use `validate_schema`/`json_validate` early to fail fast on bad input
- Leverage parallel execution — only add `depends_on` where truly needed
- Use `env()` for secrets and configuration — never hardcode tokens in flows
- Use `--verbose` when debugging to see per-task timing and outputs
