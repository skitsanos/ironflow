# IronFlow — TODO

Tracking implementation progress. Items are checked off as completed.

## Phase 1: Foundation

### 1.1 Project Scaffolding
- [x] Cargo.toml with dependencies (edition 2024, latest crate versions)
- [x] Module structure (`engine/`, `nodes/`, `lua/`, `storage/`, `cli/`, `api/`)
- [x] `lib.rs` with module exports

### 1.2 Core Types
- [x] `Context` type
- [x] `RunStatus` enum
- [x] `TaskState` struct
- [x] `RunInfo` struct
- [x] `NodeOutput` type
- [x] `StepDefinition` and `FlowDefinition`
- [x] `RetryConfig`

### 1.3 Node System
- [x] `Node` trait
- [x] `NodeRegistry`
- [x] Node config via `serde_json::Value`

### 1.4 Engine
- [x] Topological sort with cycle detection (Kahn's algorithm)
- [x] Parallel executor (tokio + semaphore)
- [x] Retry with exponential backoff
- [x] Context merge after task completion
- [x] Route-based conditional execution
- [x] Task skip on dependency failure
- [x] Duplicate step name detection at parse time
- [x] Shared `validate_dag()` for CLI and API

### 1.5 Lua Integration
- [x] Lua VM initialization with sandbox (os, io, debug removed)
- [x] `Flow` userdata with `step()` method
- [x] Step builder with `depends_on()`, `retries()`, `timeout()`, `route()`
- [x] Node factory functions exposed to Lua
- [x] Flow file loading → `FlowDefinition`
- [x] Lua table ↔ JSON conversion
- [x] Context variable interpolation (`${ctx.key}`)
- [x] `env(key)` function for reading environment variables

### 1.6 JSON State Store
- [x] `StateStore` trait
- [x] `JsonStateStore` implementation
- [x] Atomic file writes (temp + rename)
- [x] Run listing, filtering, inspect, delete

### 1.7 CLI
- [x] `run` command with `--context` and `--verbose` (step details, durations, outputs)
- [x] `validate` command (node types, dependencies, DAG cycle detection)
- [x] `list` command with `--status` filter and `--format` (table/json)
- [x] `inspect` command
- [x] `nodes` command

### 1.8 Environment Configuration
- [x] Auto-load `.env` from cwd (via `dotenvy`)
- [x] `--dotenv <path>` global CLI flag
- [x] `env()` accessible in Lua flow scripts
- [x] Function handlers (`flow:step("x", function(ctx) ... end)`) via bytecode serialization

---

## Phase 2: Core Nodes

- [x] `http_request` (+ get/post/put/delete) with auth and context interpolation
- [x] `shell_command` with timeout, process group kill, and concurrent I/O
- [x] `read_file` / `write_file` / `list_directory` (with recursive support)
- [x] `copy_file` / `move_file` / `delete_file`
- [x] `json_parse` / `json_stringify` / `select_fields`
- [x] `rename_fields`
- [x] `data_filter` / `data_transform`
- [x] `if_node` / `switch_node` with condition evaluation
- [x] `delay`
- [x] `validate_schema` (jsonschema)
- [x] `template_render`
- [x] `log`
- [x] `batch` / `deduplicate`
- [x] `hash` (SHA-256, SHA-384, SHA-512, MD5)
- [x] `code` (inline Lua execution and function handlers)
- [x] `markdown_to_html` / `html_to_markdown` (comrak + html2md)

---

## Phase 3: REST API

- [x] `serve` command (axum server with `--host`, `--port`, `--flows-dir`, `--max-body`)
- [x] `POST /flows/run` (`source`, `source_base64`, or `file`, with context)
- [x] `POST /flows/validate` (parse, check node types, deps, cycles)
- [x] `GET /runs` and `GET /runs/:id` (with `?status=` filter)
- [x] `DELETE /runs/:id` (with 404 on missing)
- [x] `GET /nodes` (list all registered nodes)
- [x] `GET /health` (version and status)
- [x] `source_base64` for escaping-free Lua submission
- [x] Mutual exclusion of source fields (reject ambiguous requests)
- [x] Configurable request body size limit (default 1 MB, `--max-body` flag)
- [x] Error response format (consistent JSON with error/details)
- [x] CORS support (permissive)
- [x] Request tracing (tower-http TraceLayer)

---

## Phase 4: Advanced Nodes

- [ ] `db_query` / `db_exec` (SQLite)
- [ ] `cache_get` / `cache_set`
- [ ] `retry_policy` / `circuit_breaker`
- [ ] `foreach`
- [ ] `send_email` / `slack_notification`
- [ ] `subworkflow` / `parallel_subworkflows`

---

## Phase 5: Polish

- [x] Structured logging (`tracing`)
- [x] Node reference docs (33+1 nodes documented)
- [x] CLI and environment variable reference
- [ ] Config file support (`ironflow.yaml`)
- [ ] Redis state store (feature flag)
- [ ] Unit + integration tests
