# IronFlow — CLI Reference

Complete reference for all commands, flags, and environment variables.

---

## Global Options

These options apply to all commands:

| Flag | Description |
|------|-------------|
| `--dotenv <PATH>` | Path to a `.env` file to load. If omitted, IronFlow auto-detects `.env` in the current directory. |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

---

## Commands

### `ironflow run <FLOW>`

Execute a workflow from a Lua flow file.

| Argument / Flag | Required | Default | Description |
|-----------------|----------|---------|-------------|
| `<FLOW>` | yes | — | Path to the `.lua` flow file |
| `-c, --context <JSON>` | no | `{}` | Initial context as a JSON string |
| `-v, --verbose` | no | off | Show step details, per-task timing, and outputs |
| `--store-dir <DIR>` | no | `data/runs` | Directory for state persistence |

```bash
ironflow run flow.lua --context '{"user": "Alice"}' --verbose
```

---

### `ironflow validate <FLOW>`

Parse and validate a flow file without executing it. Checks for:
- Unknown node types
- Missing or invalid dependencies
- DAG cycles
- Duplicate step names

| Argument / Flag | Required | Default | Description |
|-----------------|----------|---------|-------------|
| `<FLOW>` | yes | — | Path to the `.lua` flow file |

```bash
ironflow validate flow.lua
```

---

### `ironflow nodes`

List all registered node types with descriptions.

```bash
ironflow nodes
```

---

### `ironflow list`

List past workflow runs.

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `-s, --status <STATUS>` | no | all | Filter by status: `pending`, `running`, `success`, `failed`, `stalled` |
| `--store-dir <DIR>` | no | `data/runs` | State store directory |
| `--format <FORMAT>` | no | `table` | Output format: `table` or `json` |

```bash
ironflow list --status failed --format json
```

---

### `ironflow inspect <RUN_ID>`

Show full details for a specific run, including context, tasks, timing, and errors.

| Argument / Flag | Required | Default | Description |
|-----------------|----------|---------|-------------|
| `<RUN_ID>` | yes | — | The run ID (UUID) |
| `--store-dir <DIR>` | no | `data/runs` | State store directory |

```bash
ironflow inspect 3362bbd5-429e-4860-893a-34b20f43b485
```

---

### `ironflow serve`

Start the REST API server.

| Flag | Required | Default | Env Var | Description |
|------|----------|---------|---------|-------------|
| `--host <HOST>` | no | `0.0.0.0` | `HOST` | Address to bind to |
| `-p, --port <PORT>` | no | `3000` | `PORT` | Port to listen on |
| `--store-dir <DIR>` | no | `data/runs` | `IRONFLOW_STORE_DIR` | State store directory |
| `--flows-dir <DIR>` | no | — | `FLOWS_DIR` | Directory for `.lua` flow files |
| `--max-body <BYTES>` | no | `1048576` | `MAX_BODY` | Maximum request body size in bytes |

CLI flags take precedence over environment variables.
API authentication is required when binding to a non-loopback address. Set `IRONFLOW_API_KEY`; clients must send either `Authorization: Bearer <key>` or `X-API-Key: <key>`.
Browser CORS access is denied by default. Set `IRONFLOW_CORS_ORIGINS` or `cors_origins` in config to allow specific frontend origins.

```bash
# Local development
ironflow serve --host 127.0.0.1 --port 8080

# Docker / Railway / Fly.io (reads PORT from environment)
IRONFLOW_API_KEY="change-me" ironflow serve
```

### Configuration File

The `serve` command (and all other commands) can load settings from `ironflow.yaml`. Place it in the working directory for auto-detection, or specify a path with `-C`:

```bash
ironflow -C /path/to/ironflow.yaml serve
```

#### Storage Backend

IronFlow supports these state storage backends:

- **json** (default) — File-based JSON storage in `store_dir`
- **sqlite** — SQL storage in a local SQLite database
- **postgres** — SQL storage in Postgres (requires building with `--features postgres`)
- **redis** — Redis-backed storage (requires building with `--features redis`)

Configure via `ironflow.yaml`:

```yaml
store_backend: sqlite
store_url: "sqlite://data/runs/ironflow.sqlite?mode=rwc"
event_store: memory
sql_table_prefix: "ironflow_"
```

Or via environment variables (override config file):

| Variable | Default | Description |
|----------|---------|-------------|
| `IRONFLOW_STORE` | `json` | Storage backend: `json`, `sqlite`, `postgres`, or `redis` |
| `IRONFLOW_STORE_URL` | SQLite auto path for `sqlite`; required for `postgres` | SQL store URL |
| `IRONFLOW_EVENT_STORE` | `memory` | Event backend for `/runs/{id}/events`: `memory`, `sqlite`, `postgres`, or `redis` |
| `IRONFLOW_EVENT_STORE_URL` | SQLite auto path for `sqlite`; required for `postgres` | SQL event store URL |
| `IRONFLOW_SQL_TABLE_PREFIX` | `ironflow_` | SQL table/index prefix for SQLite/Postgres state and event stores |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis connection URL |
| `REDIS_PREFIX` | `ironflow:` | Key prefix for Redis keys |
| `REDIS_TTL` | — | TTL in seconds for run keys (no expiration if unset) |

When `IRONFLOW_STORE=sqlite` and no URL is configured, IronFlow creates `ironflow.sqlite` under `IRONFLOW_STORE_DIR`.
When `IRONFLOW_EVENT_STORE=sqlite` and no URL is configured, IronFlow creates `ironflow-events.sqlite` under `IRONFLOW_STORE_DIR`.
Run state and run events are configured separately, so a deployment can store run records in one backend and stream event replay from another. Redis event storage is available behind `--features redis` and uses `REDIS_URL`, `REDIS_PREFIX`, and optional `REDIS_TTL`.
For shared SQL databases, set `IRONFLOW_SQL_TABLE_PREFIX` or `sql_table_prefix` to isolate IronFlow tables. Prefixes are strictly validated and may contain only ASCII letters, digits, and underscores after deriving table names.

To build with optional backends:

```bash
cargo build --release --features postgres
cargo build --release --features redis
```

#### CORS

By default, the API does not send `Access-Control-Allow-Origin`, so browser cross-origin requests are denied. Configure exact allowed origins in `ironflow.yaml`:

```yaml
cors_origins:
  - "https://app.example.com"
  - "https://admin.example.com"
```

Or via environment variable:

```bash
IRONFLOW_CORS_ORIGINS="https://app.example.com,https://admin.example.com" ironflow serve
```

Use `IRONFLOW_CORS_ORIGINS="*"` only when intentionally allowing any browser origin.

#### API Authentication

When the API is bound to a public interface, IronFlow requires an API key:

```bash
IRONFLOW_API_KEY="change-me" ironflow serve
```

Clients can authenticate with either header:

```bash
curl http://localhost:3000/runs \
  -H "Authorization: Bearer change-me"

curl http://localhost:3000/runs \
  -H "X-API-Key: change-me"
```

To intentionally run without API authentication, set `IRONFLOW_ALLOW_UNAUTHENTICATED_API=true` or `allow_unauthenticated_api: true` in config. Loopback-only servers (`127.0.0.1`, `localhost`, `::1`) are allowed without a key for local development.

#### Run Events

`GET /runs/{id}/events` streams compact run/task lifecycle events as Server-Sent Events. Events include run/task status, step name, node type, attempts, timing, errors, and skip reasons, but never full node input/output.

```bash
curl -N http://localhost:3000/runs/<run_id>/events \
  -H "Authorization: Bearer change-me"
```

Use `?after=<event_id>` to replay events after a known event cursor.

#### Webhook Routes

Define webhook-to-flow mappings in `ironflow.yaml` to expose flows as named HTTP endpoints:

```yaml
flows_dir: "data/flows"

webhooks:
  hello: hello_world.lua            # POST /webhooks/hello
  process-order: orders/process.lua  # POST /webhooks/process-order
```

- Flow paths are resolved relative to `flows_dir`
- POST only — JSON body becomes initial workflow context
- HTTP headers are injected as `ctx._headers` (lowercase keys)
- Webhook name is injected as `ctx._webhook`

```bash
curl -X POST http://localhost:3000/webhooks/hello \
  -H "Authorization: Bearer my-token" \
  -H "Content-Type: application/json" \
  -d '{"name": "World"}'
```

---

## Environment Variables

### Server (serve command)

These are read by the `serve` command. CLI flags override them.

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Server bind address |
| `PORT` | `3000` | Server listen port |
| `IRONFLOW_STORE_DIR` | `data/runs` | State store directory |
| `IRONFLOW_STORE` | `json` | State store backend: `json`, `sqlite`, `postgres`, or `redis` |
| `IRONFLOW_STORE_URL` | — | SQL store URL for `sqlite` / `postgres` |
| `IRONFLOW_EVENT_STORE` | `memory` | Event backend for `/runs/{id}/events`: `memory`, `sqlite`, `postgres`, or `redis` |
| `IRONFLOW_EVENT_STORE_URL` | — | SQL event store URL for `sqlite` / `postgres` |
| `IRONFLOW_SQL_TABLE_PREFIX` | `ironflow_` | SQL table/index prefix for SQLite/Postgres state and event stores |
| `FLOWS_DIR` | — | Flow files directory |
| `MAX_BODY` | `1048576` | Max request body size (bytes) |
| `IRONFLOW_API_KEY` | — | API key required for non-loopback API servers |
| `IRONFLOW_ALLOW_UNAUTHENTICATED_API` | `false` | Explicitly allow unauthenticated API access |
| `IRONFLOW_CORS_ORIGINS` | — | Comma-separated allowed browser origins; use `*` to allow any origin |

### Engine

| Variable | Default | Description |
|----------|---------|-------------|
| `IRONFLOW_MAX_CONCURRENT_TASKS` | number of CPUs | Maximum tasks running in parallel per workflow execution |
| `IRONFLOW_LUA_MAX_INSTRUCTIONS` | `5000000` | Max Lua VM instructions per flow parse/code execution; `0` disables |
| `IRONFLOW_LUA_MAX_SECONDS` | `10` | Max wall-clock seconds per Lua state; `0` disables |
| `IRONFLOW_LUA_MAX_MEMORY_BYTES` | `134217728` | Max Lua VM memory per Lua state; `0` disables |
| `IRONFLOW_LUA_HOOK_INTERVAL` | `10000` | Instruction interval for budget checks |
| `IRONFLOW_LUA_GC_AFTER_EXECUTION` | `true` | Run a Lua garbage-collection cycle after flow parsing/code execution |
| `IRONFLOW_CACHE_MAX_ENTRIES` | `10000` | Max entries retained by the process-global `cache_set` / `cache_get` memory backend |
| `IRONFLOW_CACHE_DIR` | `.ironflow_cache` | Default directory for the `cache_set` / `cache_get` file backend when `cache_dir` is not set |
| `IRONFLOW_DB_MAX_ROWS` | `1000` | Max rows returned by `db_query`; `0` disables |
| `IRONFLOW_DB_MAX_RESULT_BYTES` | `10485760` | Max serialized JSON result size for `db_query`; `0` disables |
| `IRONFLOW_LLM_MAX_RESPONSE_BYTES` | `26214400` | Max LLM provider response body size; `0` disables |

Lua limits apply to flow parsing, `code` nodes, and `foreach` transform functions. For trusted dedicated-server workloads that intentionally run long Lua computations, raise the budgets or set the relevant budget to `0`.

### Dotenv

IronFlow automatically loads `.env` files at startup:

- Without `--dotenv`: auto-detects `.env` in the current working directory (silently skips if absent)
- With `--dotenv <PATH>`: loads the specified file (warns if missing)

Variables from `.env` are available in Lua flows via the `env()` function:

```lua
local api_key = env("API_KEY")
```

### User-defined variables

Any environment variable (system or `.env`) is accessible from Lua flows via `env("KEY")`. Common patterns:

```bash
# .env
OPENAI_API_KEY=sk-...
DATABASE_URL=postgres://localhost/mydb
SLACK_WEBHOOK=https://hooks.slack.com/...
```

```lua
flow:step("call", nodes.http_post({
    url = "https://api.openai.com/v1/chat/completions",
    auth = { type = "bearer", token = env("OPENAI_API_KEY") }
}))
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Error (flow load failure, validation error, execution failure) |
