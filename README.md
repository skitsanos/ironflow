# IronFlow — Workflow Automation Engine in Rust

**A lightweight, high-performance workflow engine built in Rust with Lua scripting. The open-source alternative to n8n, Airflow, and Prefect that ships as a single binary with zero dependencies.**

IronFlow is a DAG-based workflow orchestration engine designed for CI/CD pipelines, data processing, ETL jobs, API integrations, document extraction, and task automation. Define workflows in simple Lua scripts — a language with only ~20 keywords — and run them on a sandboxed, async Rust runtime with parallel step execution, retry logic, conditional routing, and a built-in REST API.

No Python. No Node.js. No Docker required. No dependency hell. Just one binary that runs on Linux, macOS, edge servers, and air-gapped environments.

```lua
local flow = Flow.new("process_order")

flow:step("validate", nodes.validate_schema({
    source_key = "order",
    schema = {
        type = "object",
        required = { "order_id", "amount" },
        properties = {
            order_id = { type = "string" },
            amount = { type = "number" }
        }
    }
}))

flow:step("charge", nodes.http_post({
    url = "https://payments.example.com/charge",
    body = { amount = "${ctx.order.amount}", order_id = "${ctx.order.order_id}" },
    auth = { type = "bearer", token = env("PAYMENT_API_KEY") }
})):depends_on("validate"):retries(3, 1.0)

flow:step("notify", nodes.log({
    message = "Order ${ctx.order.order_id} charged successfully",
    level = "info"
})):depends_on("charge")

return flow
```

```bash
ironflow run process_order.lua --context '{"order": {"order_id": "ORD-42", "amount": 99.99}}'
```

---

## Why IronFlow?

**For teams that can't use n8n, Dify, or hosted workflow platforms** — whether due to company policy, air-gapped environments, or the need for something faster and simpler.

IronFlow gives you a workflow engine that:

- **Ships as a single binary** — no runtime dependencies, no package managers, no containers required
- **Runs anywhere** — Linux, macOS, CI/CD pipelines, edge servers, embedded systems
- **Is fast** — Rust-powered execution with parallel step scheduling via DAG resolution
- **Is safe** — Sandboxed Lua can't access the filesystem or OS unless you explicitly allow it
- **Is easy to learn** — Lua has ~20 keywords. If you can write JSON, you can write flows

## The Architecture

Rust as the runtime + Lua as the scripting layer. A well-proven pattern used by Neovim, OpenResty/Nginx, Redis, and game engines like Roblox.

```
┌─────────────────────────────────────────────────────────┐
│                     Lua Flow Scripts                     │
│  flow:step("name", nodes.http_get({...}))               │
│  flow:step("process", function(ctx) ... end)            │
├─────────────────────────────────────────────────────────┤
│                    IronFlow Engine                        │
│  DAG resolution · Parallel execution · Retry/timeout     │
│  Context propagation · Conditional routing · State store │
├─────────────────────────────────────────────────────────┤
│                  41 Built-in Nodes                         │
│  HTTP · Files · Shell · Transforms · Conditionals · ...  │
│  All implemented in pure Rust for performance & safety   │
└─────────────────────────────────────────────────────────┘
```

| What | How |
|------|-----|
| Flow definitions | Lua scripts — easy to write, read, and modify |
| Node implementations | Pure Rust — fast, memory-safe, no GC pauses |
| Shared context | Lua table backed by Rust HashMap, serialized as JSON |
| DAG resolution | Topological sort with cycle detection (Kahn's algorithm) |
| Parallel execution | Steps without dependencies run concurrently via Tokio |
| State persistence | Rust trait with pluggable backends (JSON file, Redis planned) |

## Features

- **41 built-in nodes** — HTTP (GET/POST/PUT/DELETE), file I/O, shell commands, JSON transforms, foreach iteration, key-value caching (memory + file), conditional routing, schema validation, hashing, templating, Markdown conversion, document extraction (Word/PDF/HTML), database queries (SQLite via sqlx, ArangoDB via HTTP), delays, inline code execution, subworkflow composition, and PDF-to-image rendering
- **Function handlers** — pass Lua functions directly as step handlers, no boilerplate needed
- **Conditional step shorthand** — `step_if(condition, name, handler)` for concise branching
- **DAG-based scheduling** — steps run in parallel unless dependencies are declared
- **Retry with exponential backoff** — configurable per step
- **Per-step timeouts** — with proper process group cleanup on Unix
- **Conditional routing** — `if_node` and `switch_node` for branching workflows
- **Context interpolation** — `${ctx.key}` resolved everywhere, including nested JSON bodies
- **Lua globals** — `env()`, `uuid4()`, `now_rfc3339()`, `now_unix_ms()`, `json_parse()`, `json_stringify()`, `log()`, `base64_encode()`, `base64_decode()`
- **Schema validation** — JSON Schema validation to fail fast on bad input
- **REST API** — run and manage flows over HTTP (Axum-based)
- **CLI** — run, validate, inspect, and list workflows from the terminal
- **Per-step error handling** — `on_error` directive to route failures to a dedicated handler step
- **Subworkflow composition** — call other `.lua` flows as reusable modules with input/output mapping
- **Sandboxed execution** — Lua scripts run without `os`, `io`, or `debug` access

## Quick Start

### Build

```bash
git clone https://github.com/skitsanos/ironflow.git
cd ironflow
cargo build --release
```

### Run a flow

```bash
# Simple flow
ironflow run examples/01-basics/hello_world.lua --context '{"user_name": "Alice"}'

# Call OpenAI and extract the reply with a function handler
ironflow run examples/05-http/openai_with_extract.lua --context '{"prompt": "Explain recursion"}'

# Validate without executing
ironflow validate examples/03-control-flow/switch_routing.lua

# Verbose mode — see per-task timing and outputs
ironflow run examples/07-advanced/data_pipeline.lua --verbose --context '{...}'
```

### Start the REST API

```bash
ironflow serve --port 3000
```

```bash
# Run a flow via API
curl -X POST http://localhost:3000/flows/run \
  -H "Content-Type: application/json" \
  -d '{
    "source": "local flow = Flow.new(\"hello\") ...",
    "context": {"user": "Alice"}
  }'

# Or send base64-encoded Lua to avoid JSON escaping
curl -X POST http://localhost:3000/flows/run \
  -H "Content-Type: application/json" \
  -d '{
    "source_base64": "bG9jYWwgZmxvdyA9IEZsb3cubmV3KC...",
    "context": {}
  }'
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `ironflow run <file>` | Execute a workflow |
| `ironflow validate <file>` | Validate a flow without running |
| `ironflow nodes` | List all available node types |
| `ironflow list` | List past workflow runs |
| `ironflow inspect <run_id>` | Inspect a specific run |
| `ironflow serve` | Start the REST API server |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/flows/run` | Execute a flow |
| `POST` | `/flows/validate` | Validate a flow |
| `GET` | `/runs` | List all runs |
| `GET` | `/runs/{id}` | Get run details |
| `DELETE` | `/runs/{id}` | Delete a run |
| `GET` | `/nodes` | List available nodes |
| `GET` | `/health` | Health check |

## Writing Flows

### Steps and dependencies

```lua
local flow = Flow.new("my_pipeline")

-- Steps without dependencies run in parallel
flow:step("fetch_users", nodes.http_get({ url = "https://api.example.com/users" }))
flow:step("fetch_orders", nodes.http_get({ url = "https://api.example.com/orders" }))

-- This step waits for both
flow:step("merge", nodes.log({
    message = "Got ${ctx.http_data}"
})):depends_on("fetch_users", "fetch_orders")

return flow
```

### Function handlers

Write inline Lua logic as step handlers — no need for `nodes.code()`:

```lua
flow:step("transform", function(ctx)
    local total = 0
    for _, item in ipairs(ctx.items) do
        total = total + item.price * item.qty
    end
    return { order_total = total }
end):depends_on("load_items")
```

### Retries and timeouts

```lua
flow:step("call_api", nodes.http_post({
    url = "https://unreliable-api.example.com/submit",
    body = { data = "${ctx.payload}" },
    timeout = 10
})):retries(3, 1.0):timeout(30)
-- 3 retries with 1s → 2s → 4s exponential backoff
-- 30s total step timeout
```

### Conditional routing

```lua
-- Simple: step_if runs the step only when the condition is true
flow:step_if("ctx.amount > 100", "vip_discount", nodes.code({
    source = "return { discount = ctx.amount * 0.1 }"
}))

-- Full control: if_node + route for multi-branch workflows
flow:step("check", nodes.if_node({
    condition = "ctx.amount > 100",
    true_route = "premium",
    false_route = "standard"
}))

flow:step("premium_flow", nodes.log({
    message = "VIP: $${ctx.amount}"
})):depends_on("check"):route("premium")

flow:step("standard_flow", nodes.log({
    message = "Standard: $${ctx.amount}"
})):depends_on("check"):route("standard")
```

## Built-in Nodes

| Category | Nodes |
|----------|-------|
| **HTTP** | `http_request`, `http_get`, `http_post`, `http_put`, `http_delete` |
| **Files** | `read_file`, `write_file`, `copy_file`, `move_file`, `delete_file`, `list_directory` |
| **Shell** | `shell_command` |
| **Transforms** | `json_parse`, `json_stringify`, `select_fields`, `rename_fields`, `data_filter`, `data_transform`, `batch`, `deduplicate`, `foreach` |
| **Conditionals** | `if_node`, `switch_node` |
| **Validation** | `validate_schema` |
| **Markdown** | `markdown_to_html`, `html_to_markdown` |
| **Cache** | `cache_set`, `cache_get` |
| **Database** | `db_query`, `db_exec`, `arangodb_aql` |
| **Composition** | `subworkflow` |
| **Extraction** | `extract_word`, `extract_pdf`, `extract_html`, `pdf_to_image` |
| **Utility** | `log`, `delay`, `template_render`, `hash`, `code` |

See [docs/NODE_REFERENCE.md](docs/NODE_REFERENCE.md) for the complete reference with parameters and examples.

## Examples

Progressive examples from basic to advanced:

| Folder | What you'll learn |
|--------|-------------------|
| [01-basics](examples/01-basics/) | Logging, context passing, parallel execution, retries, env vars, Lua globals |
| [02-data-transforms](examples/02-data-transforms/) | JSON parse/stringify, filtering, batching, deduplication |
| [03-control-flow](examples/03-control-flow/) | If/else routing, switch/case routing, step_if shorthand |
| [04-file-operations](examples/04-file-operations/) | Read, write, copy, move, delete, list |
| [05-http](examples/05-http/) | API calls, authentication, OpenAI integration |
| [06-shell](examples/06-shell/) | Shell commands with args, env vars, timeouts |
| [07-advanced](examples/07-advanced/) | Hashing, schema validation, full data pipelines, function handlers |
| [08-extraction](examples/08-extraction/) | Word/PDF/HTML text extraction, metadata, PDF-to-image rendering |
| [09-cache](examples/09-cache/) | In-memory and file-based key-value caching with TTL |
| [10-database](examples/10-database/) | SQLite CRUD operations with db_query and db_exec |
| [11-subworkflow](examples/11-subworkflow/) | Subworkflow composition, fire-and-forget, on_error handling |
| [12-arangodb](examples/12-arangodb/) | ArangoDB AQL queries with bind variables and env-based credentials |

## Roadmap

- Redis state backend
- S3 file operations
- PostgreSQL support (via feature flag — SQLite supported now)
- Webhook triggers
- Cron scheduling
- Web UI for flow visualization

## License

MIT
