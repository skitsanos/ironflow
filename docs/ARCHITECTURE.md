# IronFlow Architecture

## Overview

IronFlow is a lightweight, high-performance workflow engine that combines a Rust execution core with Lua scripting for flow definitions. It follows the same proven pattern used by Neovim, OpenResty/Nginx, Redis, and game engines like Roblox — a fast, safe systems language for the runtime, and a minimal scripting language (~20 keywords) for the user-facing layer.

The engine provides DAG-based task scheduling with parallel execution, dependency management, retries with exponential backoff, conditional routing, and pluggable state persistence. Flows are defined in plain Lua scripts, loaded at runtime without recompilation, and executed in a sandboxed environment where scripts cannot access the filesystem or network unless explicitly granted through nodes.

IronFlow ships as a single static binary with no runtime dependencies — no Python, no Node.js, no container runtime required. It runs anywhere: CI/CD pipelines, edge servers, air-gapped environments, or as a long-running API service behind Docker, Railway, or Fly.io.

## Design Principles

1. **Rust core, Lua surface** — All nodes and the execution engine are implemented in Rust. Flows are defined in Lua scripts.
2. **Single binary** — No runtime dependencies. Ship one executable.
3. **Sandboxed execution** — Lua scripts run in a restricted environment. No filesystem or network access unless explicitly granted through nodes.
4. **Async-first** — The engine uses `tokio` for async execution. Nodes can be async or sync.
5. **Pluggable persistence** — State storage is trait-based. JSON file storage ships by default; Redis is optional.

## System Layers

```
┌─────────────────────────────────────────────┐
│              CLI / REST API                  │
│         (clap + axum)                        │
├─────────────────────────────────────────────┤
│            Lua Runtime (mlua)                │
│  ┌───────────────────────────────────────┐   │
│  │  Flow definitions (.lua files)        │   │
│  │  - step(), depends_on(), retries()    │   │
│  │  - Access to node functions           │   │
│  │  - Context read/write via ctx table   │   │
│  │  - env() for environment variables    │   │
│  └───────────────────────────────────────┘   │
├─────────────────────────────────────────────┤
│           Workflow Engine                    │
│  ┌──────────┐ ┌──────────┐ ┌────────────┐   │
│  │ DAG      │ │ Executor │ │ Retry      │   │
│  │ Builder  │ │ (tokio)  │ │ Manager    │   │
│  └──────────┘ └──────────┘ └────────────┘   │
├─────────────────────────────────────────────┤
│              Node Registry                   │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌───────────┐   │
│  │ HTTP │ │Shell │ │File  │ │Transform  │   │
│  │      │ │      │ │ Ops  │ │& Utility  │   │
│  └──────┘ └──────┘ └──────┘ └───────────┘   │
├─────────────────────────────────────────────┤
│           State Persistence                  │
│  ┌──────────────┐ ┌──────────┐ ┌───────────┐  │
│  │ JSON Store   │ │Null Store│ │Redis (opt)│  │
│  └──────────────┘ └──────────┘ └───────────┘  │
└─────────────────────────────────────────────┘
```

## Key Components

### 1. Workflow Engine (`engine/`)

The core execution engine responsible for:
- Parsing Lua flow definitions into a DAG
- Topological sorting with cycle detection (Kahn's algorithm)
- Parallel execution of independent tasks via `tokio` tasks
- Concurrency control via semaphores
- Context management (shared HashMap passed between nodes)
- Duplicate step name detection at parse time
- On-error handlers (`on_error()` routes failures to recovery steps)
- Dependency-failure propagation (downstream steps are skipped)

### 2. Node System (`nodes/`)

Each node is a Rust struct implementing the `Node` trait:

```rust
#[async_trait]
pub trait Node: Send + Sync {
    fn node_type(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput>;
}
```

Nodes are registered in a `NodeRegistry` and exposed to Lua as callable factory functions. 96 built-in nodes are provided across HTTP, shell, file, S3, S3 vector, MCP, data transform, iteration, caching, conditional, timing, code execution, markdown, XML, YAML, HTML sanitization, date/time, encoding, document extraction, image processing, database, AI, subworkflow, notification, and utility categories. The `pdf_to_image` and `pdf_thumbnail` nodes require the native `pdfium` library at runtime.

### 3. Lua Runtime (`lua/`)

- Uses `mlua` crate with Lua 5.4
- Flow definitions return a `Flow` object with steps and dependencies
- Context variable interpolation (`${ctx.key}` with nested dot-path support)
- Sandbox restricts access — `os`, `io`, `debug`, `loadfile`, `dofile` are removed
- `env(key)` function exposed for reading environment variables
- Function handlers — Lua functions passed directly as step handlers are compiled to bytecode and executed as `code` nodes

### 4. State Store (`storage/`)

```rust
#[async_trait]
pub trait StateStore: Send + Sync {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> Result<()>;
    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<()>;
    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> Result<()>;
    async fn get_ctx(&self, run_id: &str) -> Result<Context>;
    async fn update_ctx(&self, run_id: &str, updates: &Context) -> Result<()>;
    async fn get_run_info(&self, run_id: &str) -> Result<RunInfo>;
    async fn list_runs(&self, filter: Option<RunStatus>) -> Result<Vec<RunInfo>>;
    async fn delete_run(&self, run_id: &str) -> Result<()>;
}
```

Implementations:
- **JsonStateStore** — File-based, one JSON file per run with atomic writes and `RwLock`
- **NullStateStore** — In-memory, transient (used by subworkflow nodes)
- **RedisStateStore** — Redis-backed (optional, `redis` feature flag). Uses a Redis Hash per run with a Set index for efficient listing. Supports configurable key prefix, TTL, and auto-reconnecting connection pool.

Backend selection is controlled by the `store_backend` config field or `STORE_BACKEND` environment variable.

### 5. CLI (`cli/`)

Built with `clap`. Commands:
- `ironflow run <flow.lua>` — Execute a flow with `--context`, `--verbose`, `--store-dir`
- `ironflow validate <flow.lua>` — Check flow for errors (node types, dependencies, cycles)
- `ironflow list` — List past runs with `--status` filter and `--format` (table/json)
- `ironflow inspect <run_id>` — Show run details as JSON
- `ironflow nodes` — List available node types
- `ironflow serve` — Start REST API server with `--host`, `--port`, `--flows-dir`, `--max-body`

Global flags:
- `--dotenv <path>` — Load environment variables from a specific file

### 6. REST API (`api/`)

Built with `axum`. Endpoints:
- `POST /flows/run` — Submit a flow for execution (via `source`, `source_base64`, or `file`)
- `POST /flows/validate` — Validate a flow without executing
- `GET /runs` — List runs with optional `?status=` filter
- `GET /runs/:id` — Get full run details (context, tasks, timing)
- `DELETE /runs/:id` — Delete a run record
- `GET /nodes` — List available nodes with descriptions
- `POST /webhooks/{name}` — Execute a webhook-mapped flow (configured in `ironflow.yaml`)
- `GET /health` — Version and status check

Features:
- Exactly one source field required per request (mutual exclusion enforced)
- Base64 flow source support (`source_base64`) for escaping-free submission
- Configurable request body size limit (default 1 MB, `--max-body` flag)
- CORS support (permissive)
- Request tracing via `tower-http`

## Data Flow

```
1. User writes flow.lua
2. CLI/API loads flow.lua into Lua VM
3. Lua script calls step()/depends_on() → builds FlowDefinition
4. Engine validates: duplicate names, unknown nodes, DAG cycles
5. Topological sort → execution phases
6. For each phase, run ready tasks concurrently:
   a. Check route conditions and dependency failures
   b. Resolve node from registry
   c. Pass config + context to node.execute()
   d. Merge output into shared context
   e. Update state store
   f. On failure: retry with exponential backoff, route to on_error handler, or mark failed
7. Final status written to state store
8. Context returned to caller
```

## Context Model

Context is a `HashMap<String, serde_json::Value>` that flows through the entire workflow:

- Each node receives the full context (read-only snapshot)
- Node output (a map) is merged into context after execution
- Keys prefixed with `_` are reserved for engine internals (routes, conditions)
- Webhook requests inject `_headers` (HTTP headers map) and `_webhook` (webhook name)
- In Lua configs, context is accessed via `${ctx.key}` interpolation

## Concurrency Model

- Per-workflow task semaphore limits concurrent task executions
- Configurable via environment variable:
  - `IRONFLOW_MAX_CONCURRENT_TASKS` (default: num_cpus)
- Shell commands spawn in dedicated process groups for clean timeout cleanup
