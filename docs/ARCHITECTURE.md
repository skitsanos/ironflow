# IronFlow Architecture

## Overview

IronFlow is a lightweight workflow engine that provides deterministic task execution with dependency management, retries, and pluggable state persistence. It is a Rust reimplementation of the Python [microflow](https://github.com/skitsanos/microflow) engine, with Lua as the flow definition language.

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
│  │      │ │      │ │ Ops  │ │           │   │
│  └──────┘ └──────┘ └──────┘ └───────────┘   │
├─────────────────────────────────────────────┤
│           State Persistence                  │
│  ┌──────────────┐  ┌───────────────────┐     │
│  │ JSON Store   │  │ Redis Store (opt) │     │
│  └──────────────┘  └───────────────────┘     │
└─────────────────────────────────────────────┘
```

## Key Components

### 1. Workflow Engine (`engine/`)

The core execution engine responsible for:
- Parsing Lua flow definitions into a DAG
- Topological sorting (Kahn's algorithm) for execution order
- Parallel execution of independent tasks via `tokio` tasks
- Concurrency control via semaphores
- Context management (shared HashMap exposed as Lua table)

### 2. Node System (`nodes/`)

Each node is a Rust struct implementing the `Node` trait:

```rust
#[async_trait]
pub trait Node: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, ctx: &mut Context) -> Result<NodeOutput>;
}
```

Nodes are registered in a `NodeRegistry` and exposed to Lua as callable functions.

### 3. Lua Runtime (`lua/`)

- Uses `mlua` crate with Lua 5.4
- Flow definitions return a `Flow` object with steps and dependencies
- Context is a Lua table backed by `serde_json::Value` for easy Rust interop
- Sandbox restricts access to only workflow-related functions

### 4. State Store (`storage/`)

```rust
#[async_trait]
pub trait StateStore: Send + Sync {
    async fn init_run(&self, run_id: &str, ctx: &Context) -> Result<()>;
    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<()>;
    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> Result<()>;
    async fn get_ctx(&self, run_id: &str) -> Result<Context>;
    async fn update_ctx(&self, run_id: &str, updates: &Context) -> Result<()>;
    async fn get_run_info(&self, run_id: &str) -> Result<RunInfo>;
    async fn list_runs(&self, filter: Option<RunStatus>) -> Result<Vec<RunInfo>>;
    async fn delete_run(&self, run_id: &str) -> Result<()>;
}
```

### 5. CLI (`cli/`)

Built with `clap`. Commands:
- `ironflow run <flow.lua>` — Execute a flow
- `ironflow validate <flow.lua>` — Check flow for errors without running
- `ironflow list` — List past runs
- `ironflow inspect <run_id>` — Show run details
- `ironflow serve` — Start REST API server

### 6. REST API (`api/`)

Built with `axum`. Endpoints:
- `POST /flows/run` — Submit a flow for execution
- `GET /runs` — List runs
- `GET /runs/:id` — Get run status and details
- `DELETE /runs/:id` — Delete a run record
- `GET /nodes` — List available nodes

## Data Flow

```
1. User writes flow.lua
2. CLI/API loads flow.lua into Lua VM
3. Lua script calls step()/depends_on() → builds FlowDefinition
4. Engine converts FlowDefinition → DAG
5. Topological sort → execution phases
6. For each phase, run ready tasks concurrently:
   a. Resolve node from registry
   b. Pass context to node.execute()
   c. Merge output into context
   d. Update state store
   e. On failure: retry with backoff or mark failed
7. Final status written to state store
8. Context returned to caller
```

## Context Model

Context is a `HashMap<String, serde_json::Value>` that flows through the entire workflow:

- Each node receives the full context
- Node output (a map) is merged into context after execution
- Keys prefixed with `_` are reserved for engine internals
- In Lua, context is accessed as `ctx.key` or `ctx["key"]`

## Concurrency Model

- Global workflow semaphore limits concurrent workflow executions
- Per-workflow task semaphore limits concurrent task executions
- Both configurable via environment variables:
  - `IRONFLOW_MAX_CONCURRENT_WORKFLOWS` (default: num_cpus)
  - `IRONFLOW_MAX_CONCURRENT_TASKS` (default: num_cpus)
