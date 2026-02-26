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
| `--store-dir <DIR>` | no | `data/runs` | `STORE_DIR` | State store directory |
| `--flows-dir <DIR>` | no | — | `FLOWS_DIR` | Directory for `.lua` flow files |
| `--max-body <BYTES>` | no | `1048576` | `MAX_BODY` | Maximum request body size in bytes |

CLI flags take precedence over environment variables.

```bash
# Local development
ironflow serve --port 8080

# Docker / Railway / Fly.io (reads PORT from environment)
ironflow serve
```

---

## Environment Variables

### Server (serve command)

These are read by the `serve` command. CLI flags override them.

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Server bind address |
| `PORT` | `3000` | Server listen port |
| `STORE_DIR` | `data/runs` | State store directory |
| `FLOWS_DIR` | — | Flow files directory |
| `MAX_BODY` | `1048576` | Max request body size (bytes) |

### Engine

| Variable | Default | Description |
|----------|---------|-------------|
| `IRONFLOW_MAX_CONCURRENT_TASKS` | number of CPUs | Maximum tasks running in parallel per workflow execution |

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
