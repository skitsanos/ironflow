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

## Phase 2: Core Nodes ✅

Implement the essential node types. Each node is a Rust struct implementing `Node`. **41 nodes total.**

### 2.1 HTTP Nodes ✅
- [x] `http_request` — Generic HTTP with method, url, headers, body, auth, timeout
- [x] `http_get`, `http_post`, `http_put`, `http_delete` — Convenience wrappers
- [x] Auth support: Bearer, Basic, API Key
- [x] Response parsing (JSON by default, fallback to string)
- [x] Context variable interpolation in URLs, headers, and auth tokens

### 2.2 Shell Nodes ✅
- [x] `shell_command` — Execute command, capture stdout/stderr/exit code
- [x] Timeout support with process group kill (prevents orphan processes)
- [x] Concurrent stdout/stderr reading (prevents pipe-buffer deadlocks)
- [x] Environment variable passthrough
- [x] Working directory configuration

### 2.3 File Operation Nodes ✅
- [x] `read_file` — Read file contents (text or binary as base64)
- [x] `write_file` — Write/append to file (text or binary from base64, with `source_key` for context data)
- [x] `copy_file` — Copy a file to a new location
- [x] `move_file` — Move/rename a file
- [x] `delete_file` — Delete a file
- [x] `list_directory` — List directory entries (with full recursive support)

### 2.4 Data Transform Nodes ✅
- [x] `json_parse`, `json_stringify`
- [x] `select_fields` — Pick specific fields from an object
- [x] `rename_fields` — Rename fields in an object via mapping
- [x] `data_filter` — Filter array items by field condition (eq, neq, gt, lt, gte, lte, contains, exists)
- [x] `data_transform` — Map/rename fields across objects or arrays

### 2.5 Conditional Nodes ✅
- [x] `if_node` — Evaluate condition, set route in context
- [x] `switch_node` — Multi-case routing
- [x] Route-based task skipping (via `route()` on step builder)

### 2.6 Timing Nodes ✅
- [x] `delay` — Sleep for specified duration

### 2.7 Utility Nodes ✅
- [x] `validate_schema` — JSON Schema validation (via `jsonschema` crate)
- [x] `template_render` — String template interpolation with `${ctx.key}`
- [x] `log` — Write to workflow log with configurable level
- [x] `batch` — Split an array into chunks of specified size
- [x] `deduplicate` — Remove duplicates from array (by field or full value)
- [x] `hash` — Compute hash (SHA-256, SHA-384, SHA-512, MD5) of strings or context values
- [x] `code` — Execute inline Lua code or function handlers with sandboxed context access
- [x] `markdown_to_html` — Markdown to HTML conversion (CommonMark + GFM, optional sanitization)
- [x] `html_to_markdown` — HTML to Markdown conversion (best-effort, lossy on complex HTML)

### 2.8 Document Extraction Nodes ✅
- [x] `extract_word` — Extract text and metadata from Word (.docx)
- [x] `extract_pdf` — Extract text and metadata from PDF
- [x] `extract_html` — Extract text and metadata from HTML
- [x] `pdf_to_image` — Render PDF pages to images (optional, requires `pdf-render` feature flag)

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

### 3.2 Redis State Store (optional feature)
- [ ] Implement `RedisStateStore` behind `redis` cargo feature flag
- [ ] Same trait interface as JSON store
- [ ] Key prefix configuration
- [ ] Connection pooling

---

## Phase 4: Advanced Nodes

### 4.1 Integration Nodes
- [x] `db_query`, `db_exec` — SQLite support via `sqlx` (PostgreSQL via `postgres` feature flag)
- [x] `cache_get`, `cache_set` — In-memory and file-based cache with TTL
- [x] `foreach` — Array iteration with Lua function transforms and null filtering

### 4.2 Subworkflow Nodes
- [x] `subworkflow` — Load and execute another `.lua` flow as a reusable module
- [x] Context mapping (input_keys, output_keys) for clean interfaces between flows
- [ ] `parallel_subworkflows` — Concurrent subworkflow execution

### 4.3 Notification Nodes
- [ ] `send_email` — SMTP email via `lettre`
- [ ] `slack_notification` — Webhook-based Slack messages

### 4.4 Control Plane Nodes
- [ ] `metrics_emit` — In-memory metrics collection
- [ ] `queue_publish`, `queue_consume` — In-memory queue

---

## Phase 5: Polish & Production Readiness

### 5.1 Observability ✅
- [x] Structured logging via `tracing` crate
- [x] Per-task timing in state store (started/finished timestamps)
- [x] Workflow execution summary on completion (CLI prints task statuses)

### 5.2 Configuration
- [ ] Config file support (`ironflow.toml`)
- [ ] Environment variable overrides
- [ ] Storage backend selection via config

### 5.3 Testing ✅
- [x] Unit tests for each node (37 tests in `test_nodes`)
- [x] Integration tests for engine (12 tests in `test_engine` — sequential, parallel, deps, timeout, on_error, routing, file I/O)
- [x] Lua flow parsing tests (16 tests in `test_lua_runtime`)
- [x] State store tests (15 tests in `test_state_stores` — NullStateStore and JsonStateStore)
- [x] API endpoint tests (7 tests in `test_api` — run, validate, list, delete, base64, nodes)
- [x] Interpolation unit tests (4 tests in `lib.rs`)

### 5.4 Documentation ✅
- [x] Node reference with individual per-node files (`docs/nodes/`)
- [x] Lua flow writing guide (`docs/LUA_FLOW_GUIDE.md`)
- [x] CLI and environment variable reference (`docs/CLI_REFERENCE.md`)
- [x] Examples organized by category with README (11 folders, 36 examples)
- [ ] API reference

### 5.5 Infrastructure ✅
- [x] GitHub Actions CI (check, clippy, fmt, test, build, validate examples)
- [x] Shared Lua sandbox module (`lua_sandbox.rs`) for consistent VM setup

---

## Dependency Map

```
Phase 1 ✅ ──→ Phase 2 ✅ ──→ Phase 3 ✅
                   │               │
                   └────→ Phase 4  │
                             │     │
                             └──→ Phase 5 (partial ✅)
```

Phases 1-3 are complete (39+1 nodes, full CLI, REST API). Phase 4 partially done (db, cache, foreach, subworkflow). Phase 5 mostly done — testing complete (103 tests), config and Redis remaining.
