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
    headers = { Authorization = "Bearer ${ctx.token}" }
}))
```

Each step has:
- **name** (string) — unique identifier within the flow
- **node** (node config) — the node type and its configuration

## Dependencies

Steps run in parallel by default. Use `depends_on()` to enforce ordering:

```lua
flow:step("process", nodes.data_transform({
    expression = "item.value * 2"
})):depends_on("fetch_data")
```

Multiple dependencies:

```lua
flow:step("merge", nodes.data_transform({
    -- merge results from both
})):depends_on("fetch_users", "fetch_orders")
```

## Retries

Configure retry behavior per step:

```lua
flow:step("call_api", nodes.http_post({
    url = "https://api.example.com/submit",
    body = { data = "ctx.payload" }
})):retries(3, 1.0)  -- max 3 retries, 1s initial backoff
```

The backoff is exponential: 1s → 2s → 4s.

## Timeout

Set a per-step timeout:

```lua
flow:step("slow_op", nodes.shell_command({
    cmd = "long-running-script.sh"
})):timeout(30)  -- 30 seconds
```

## Context

Context is a shared key-value store that flows through all steps:

```lua
-- Initial context is passed via CLI or API:
-- ironflow run flow.lua --context '{"user_id": "123"}'

-- Steps read from context via their config:
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
    template = "User email: ${ctx.user.email}"
})
```

## Conditional Execution

Use conditional nodes to branch:

```lua
flow:step("check", nodes.if_node({
    condition = "ctx.amount > 100",
    true_route = "high_value",
    false_route = "normal"
}))

flow:step("high_value_handler", nodes.http_post({
    url = "https://api.example.com/vip",
    body = { amount = "ctx.amount" }
})):depends_on("check"):route("high_value")

flow:step("normal_handler", nodes.log({
    message = "Standard processing"
})):depends_on("check"):route("normal")
```

## Available Nodes

See [NODE_REFERENCE.md](NODE_REFERENCE.md) for the complete list of nodes and their configuration options.

## Complete Example

```lua
-- order_processing.lua
local flow = Flow.new("process_order")

-- Validate the incoming order
flow:step("validate", nodes.validate_schema({
    source_key = "order",
    schema = {
        type = "object",
        required = { "order_id", "user_id", "items", "total" },
        properties = {
            order_id = { type = "string" },
            user_id = { type = "string" },
            items = { type = "array" },
            total = { type = "number" }
        }
    }
}))

-- Fetch user details (runs after validation)
flow:step("fetch_user", nodes.http_get({
    url = "https://api.example.com/users/${ctx.user_id}",
    output_key = "user"
})):depends_on("validate")

-- Check inventory (runs after validation, parallel with fetch_user)
flow:step("check_inventory", nodes.http_post({
    url = "https://api.example.com/inventory/check",
    body = { items = "ctx.items" },
    output_key = "inventory"
})):depends_on("validate")

-- Process payment (needs both user and inventory)
flow:step("charge", nodes.http_post({
    url = "https://payments.example.com/charge",
    body = {
        amount = "ctx.total",
        email = "ctx.user.email"
    },
    output_key = "payment"
})):depends_on("fetch_user", "check_inventory"):retries(3, 2.0)

-- Send confirmation
flow:step("notify", nodes.send_email({
    to = "ctx.user.email",
    subject = "Order ${ctx.order_id} confirmed",
    body = "Your order has been processed. Payment ID: ${ctx.payment.id}"
})):depends_on("charge")

return flow
```

## Tips

- Keep flows focused — one flow per logical workflow
- Use meaningful step names — they appear in logs and state
- Set retries on external calls (HTTP, email, etc.)
- Set timeouts on potentially slow operations
- Use `validate_schema` early to fail fast on bad input
- Leverage parallel execution — only add `depends_on` where truly needed
