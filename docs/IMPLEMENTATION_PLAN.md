# IronFlow Implementation Plan

## Phase 1: Foundation ✅

The core engine, minimal node set, and CLI. Goal: execute a simple multi-step flow from a Lua file.

### 1.1 Project Scaffolding ✅
- [x] Set up Cargo workspace structure (edition 2024)
- [x] Add core dependencies: `tokio`, `mlua`, `serde`, `serde_json`, `clap`, `anyhow`, `thiserror`, `uuid`, `chrono`
- [x] Define module structure: `engine/`, `nodes/`, `lua/`, `storage/`, `cli/`, `api/`
- [x] Create `lib.rs` with public module exports

### 1.2 Context & Types ✅
- [x] Define `Context` type (`HashMap<String, serde_json::Value>`)
- [x] Define `RunStatus` enum: `Pending`, `Running`, `Success`, `Failed`, `Stalled`
- [x] Define `TaskState` struct: name, status, attempt, input, output, error, timestamps
- [x] Define `RunInfo` struct: id, status, started, finished, context, tasks
- [x] Define `NodeOutput` type (alias for context map)

### 1.3 Node Trait & Registry ✅
- [x] Define `Node` trait with `execute()`, `node_type()`, `description()`
- [x] Implement `NodeRegistry` (HashMap of name → `Arc<dyn Node>`)
- [x] Node configuration via `serde_json::Value` (passed from Lua)

### 1.4 DAG & Execution Engine ✅
- [x] Define `StepDefinition` struct: name, node_type, config, dependencies, retry config
- [x] Define `FlowDefinition` struct: name, steps, metadata
- [x] Implement topological sort with cycle detection (Kahn's algorithm)
- [x] Implement parallel executor using `tokio::spawn` + `Semaphore`
- [x] Implement retry logic with exponential backoff
- [x] Context merging after each task completion
- [x] Route-based conditional task execution
- [x] Task skip on dependency failure
- [x] Duplicate step name detection at parse time
- [x] Shared `validate_dag()` method on `FlowDefinition` for CLI and API

### 1.5 Lua Integration ✅
- [x] Initialize `mlua::Lua` with sandbox settings (os, io, debug removed)
- [x] Expose `Flow` userdata to Lua (step, depends_on, retries, timeout, route)
- [x] Expose node factory functions to Lua (e.g., `nodes.http_get({...})`)
- [x] Load and parse `.lua` flow files → `FlowDefinition`
- [x] Lua table ↔ JSON conversion (custom `lua_table_to_json` / `lua_value_to_json`)
- [x] Context variable interpolation (`${ctx.key}` with nested dot-path support)
- [x] `env(key)` function exposed to Lua for reading environment variables
- [x] `base64_encode(str)` / `base64_decode(str)` Lua globals (shared sandbox module)
- [x] Function handlers — pass Lua functions directly as step handlers (bytecode serialization)
- [x] `step_if(condition, name, handler)` — conditional step shorthand (syntactic sugar over `if_node` + `route`)

### 1.6 JSON State Store ✅
- [x] Implement `StateStore` trait
- [x] Implement `JsonStateStore` (file-based, `data/runs/{run_id}.json`)
- [x] Atomic writes (write to temp, rename)
- [x] Thread-safe access via `tokio::sync::RwLock`

### 1.7 CLI ✅
- [x] `ironflow run <flow.lua>` — load, execute, print result
- [x] `ironflow validate <flow.lua>` — parse and check DAG, report errors
- [x] `--context` flag to pass initial context as JSON string
- [x] `--verbose` flag for detailed execution output (step details, task durations, outputs)
- [x] Pretty-printed output with task status indicators (✓, ✗, ⊘, ⟳, ○)
- [x] `ironflow list` — List past runs with `--status` filter and `--format` (table/json)
- [x] `ironflow inspect <run_id>` — Show detailed run info as JSON
- [x] `ironflow nodes` — List available nodes with descriptions

### 1.8 Environment Configuration ✅
- [x] Auto-load `.env` file from current working directory (via `dotenvy`)
- [x] `--dotenv <path>` global CLI flag for custom dotenv file path
- [x] Environment variables accessible from Lua via `env(key)` function

---

## Phase 2: Nodes ✅

78 built-in nodes across HTTP, shell, file, S3, data transforms, conditionals, caching, database, AI, notifications, composition, S3 vector, and utility categories. Each node is a Rust struct implementing the `Node` trait.

See [NODE_REFERENCE.md](NODE_REFERENCE.md) for the complete list with parameters, context output, and Lua examples.

---

## Phase 3: REST API & Persistence ✅

### 3.1 REST API Server ✅
- [x] `ironflow serve` command with `--host`, `--port`, `--flows-dir`, `--max-body` flags
- [x] `POST /flows/run` — Accept `source`, `source_base64`, or `file`, with initial context
- [x] `POST /flows/validate` — Validate without executing (node types, deps, DAG cycles)
- [x] `GET /runs` — List runs with optional `?status=` filter (summary view)
- [x] `GET /runs/:id` — Get full run info (context, tasks, timing)
- [x] `DELETE /runs/:id` — Delete run (404 on missing)
- [x] `GET /nodes` — List registered nodes with descriptions
- [x] `GET /health` — Version and status check
- [x] `source_base64` field for escaping-free Lua submission
- [x] Mutual exclusion — reject requests with multiple source fields
- [x] Configurable request body size limit (default 1 MB, `--max-body` flag)
- [x] Error responses with consistent JSON format (`error` + optional `details`)
- [x] CORS support (permissive, via `tower-http`)
- [x] Request tracing (via `tower-http` TraceLayer)

### 3.2 Redis State Store
- [ ] Implement `RedisStateStore` behind `redis` cargo feature flag
- [ ] Same trait interface as JSON store
- [ ] Key prefix configuration
- [ ] Connection pooling

---

## Phase 4: Advanced Features

### 4.1 Subworkflow Composition
- [x] `subworkflow` — Load and execute another `.lua` flow as a reusable module
- [x] Context mapping (input_keys, output_keys) for clean interfaces between flows
- [ ] `parallel_subworkflows` — Concurrent subworkflow execution

---

## Phase 5: Polish & Production Readiness

### 5.1 Observability ✅
- [x] Structured logging via `tracing` crate
- [x] Per-task timing in state store (started/finished timestamps)
- [x] Workflow execution summary on completion (CLI prints task statuses)

### 5.2 Configuration
- [x] Config file support (`ironflow.yaml`) — auto-detected in cwd or via `--config` flag
- [x] Environment variable overrides (CLI flags > config file > env vars > defaults)
- [x] Webhook routes via config — `webhooks:` map in `ironflow.yaml` creates `POST /webhooks/{name}` endpoints
- [ ] Storage backend selection via config

### 5.3 Testing ✅
- [x] Unit tests for each node (in `test_nodes` and domain-specific suites)
- [x] Integration tests for engine (12 tests in `test_engine`)
- [x] Lua flow parsing tests (16 tests in `test_lua_runtime`)
- [x] State store tests (15 tests in `test_state_stores`)
- [x] API endpoint tests (7 tests in `test_api`)
- [x] Interpolation unit tests (4 tests in `lib.rs`)
- [x] Hundreds of tests across tens of test files

### 5.4 Documentation ✅
- [x] Node reference with individual per-node files (`docs/nodes/`)
- [x] Lua flow writing guide (`docs/LUA_FLOW_GUIDE.md`)
- [x] CLI and environment variable reference (`docs/CLI_REFERENCE.md`)
- [x] Examples organized by category with README (16 folders, 71+ examples)

### 5.5 Infrastructure ✅
- [x] GitHub Actions CI (check, clippy, fmt, test, build, validate examples) — path-filtered to skip docs-only changes
- [x] GitHub Actions Release workflow — builds Linux (musl), macOS (x86_64 + aarch64), Windows on version tags
- [x] Shared Lua sandbox module (`lua_sandbox.rs`) for consistent VM setup
