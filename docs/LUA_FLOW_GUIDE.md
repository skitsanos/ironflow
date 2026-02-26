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
})):timeout(30)  -- 30 second step-level timeout
```

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

-- Step outputs are merged back into context.
-- After "greet" runs, ctx.greeting = "Hello, Alice!"
```

### Context variable interpolation

Strings containing `${ctx.key}` are resolved at runtime:

```lua
nodes.http_get({
    url = "https://api.example.com/users/${ctx.user_id}"
})
```

Nested access with dots:

```lua
nodes.template_render({
    template = "User email: ${ctx.user.email}",
    output_key = "info"
})
```

## Environment Variables

Use `env(key)` to read environment variables in Lua. Works with system env vars and values from `.env` files:

```lua
local api_key = env("API_KEY")
local db_url = env("DATABASE_URL") or "sqlite://default.db"

flow:step("call_api", nodes.http_get({
    url = "https://api.example.com/data",
    auth = { type = "bearer", token = env("API_TOKEN") },
    output_key = "api"
}))
```

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

See [NODE_REFERENCE.md](NODE_REFERENCE.md) for the complete list of 28 nodes and their configuration options.

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

The code runs in a sandboxed Lua VM (no `os`, `io`, `debug` access). Return a table to merge key-value pairs into context, or a single value (stored under `result`).

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

The function receives `ctx` (the full workflow context) as its argument and returns a table of key-value pairs to merge into context. Under the hood, the function is compiled to bytecode at parse time and executed as a `code` node — so the same sandbox rules apply. `env()` works inside handlers.

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
- Use `validate_schema` early to fail fast on bad input
- Leverage parallel execution — only add `depends_on` where truly needed
- Use `env()` for secrets and configuration — never hardcode tokens in flows
- Use `--verbose` when debugging to see per-task timing and outputs
