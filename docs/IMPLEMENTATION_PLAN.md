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

96 built-in nodes across HTTP, shell, file, S3, MCP, data transforms, conditionals, caching, database, AI, notifications, composition, S3 vector, XML, YAML, HTML sanitization, date/time, encoding, and utility categories. Each node is a Rust struct implementing the `Node` trait.

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
- [x] API key authentication for non-loopback servers via `IRONFLOW_API_KEY`
- [x] Configurable CORS support via `IRONFLOW_CORS_ORIGINS` / `cors_origins`
- [x] Request tracing (via `tower-http` TraceLayer)
- [x] Lua instruction, wall-clock, memory, and GC controls for flow parsing and Lua nodes

### 3.2 Redis State Store ✅
- [x] Implement `RedisStateStore` behind `redis` cargo feature flag
- [x] Same trait interface as JSON store
- [x] Key prefix configuration (`redis_prefix` in config or `REDIS_PREFIX` env var)
- [x] Connection pooling (via `ConnectionManager` with auto-reconnect)
- [x] Optional TTL for automatic run expiration (`redis_ttl` in config or `REDIS_TTL` env var)
- [x] `create_store()` factory function for backend selection (config + env var)
- [x] `AppState.store` refactored to `Arc<dyn StateStore>` for runtime backend selection
- [x] 8 integration tests against real Redis

### 3.3 SQL State Store ✅
- [x] `SqlStateStore` for SQLite/Postgres via `sqlx::AnyPool`
- [x] Separate SQL tables for runs and tasks to avoid rewriting full run records on task updates
- [x] Backend selection via `IRONFLOW_STORE=json|sqlite|postgres|redis`
- [x] SQL store URL via `IRONFLOW_STORE_URL` / `store_url`
- [x] SQL table prefix via `IRONFLOW_SQL_TABLE_PREFIX` / `sql_table_prefix` for shared SQLite/Postgres databases

### 3.4 Run Event Streaming ✅
- [x] Define compact `RunEvent` payloads for run/task lifecycle monitoring; include step name, `node_type`, attempt, status, timing, and error metadata, but never full node input/output.
- [x] Add separate event backend selection via `IRONFLOW_EVENT_STORE=memory|sqlite|postgres|redis` and `IRONFLOW_EVENT_STORE_URL`; do not reuse `IRONFLOW_STORE` so deployments can store runs and events in different systems.
- [x] Implement in-memory event store for single-instance/local deployments.
- [x] Implement SQL event store for SQLite/Postgres shared event backends.
- [x] Apply the shared SQL table prefix to SQL event tables.
- [x] Implement Redis event store behind the `redis` cargo feature flag, using `REDIS_URL`, `REDIS_PREFIX`, and optional `REDIS_TTL`.
- [x] Emit events from the workflow engine next to run/task state transitions.
- [x] Add `GET /runs/{id}/events` SSE endpoint with replay support from the selected event backend.
- [x] Defer Redis Streams, NATS, Kafka/Redpanda, and other event backends to a later phase.

---

## Phase 4: Advanced Features

### 4.1 Subworkflow Composition ✅
- [x] `subworkflow` — Load and execute another `.lua` flow as a reusable module
- [x] Context mapping (input_keys, output_keys) for clean interfaces between flows
- [x] `parallel_subworkflows` — Concurrent subworkflow execution with per-flow input mapping, error handling modes (`fail_fast` / `ignore`), and ordered result collection

---

## Phase 5: Polish & Production Readiness

### 5.1 Observability ✅
- [x] Structured logging via `tracing` crate
- [x] Per-task timing in state store (started/finished timestamps)
- [x] Workflow execution summary on completion (CLI prints task statuses)

### 5.2 Configuration ✅
- [x] Config file support (`ironflow.yaml`) — auto-detected in cwd or via `--config` flag
- [x] Environment variable overrides (CLI flags > config file > env vars > defaults)
- [x] Webhook routes via config — `webhooks:` map in `ironflow.yaml` creates `POST /webhooks/{name}` endpoints
- [x] Storage backend selection via config — `store_backend`, `store_url`, `event_store`, `event_store_url`, `sql_table_prefix` fields in `ironflow.yaml` (see `src/cli/config.rs:26-34`); env-var equivalents `IRONFLOW_STORE` / `IRONFLOW_STORE_URL` / `IRONFLOW_EVENT_STORE` / `IRONFLOW_EVENT_STORE_URL` / `IRONFLOW_SQL_TABLE_PREFIX`

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
- [x] Examples organized by category with README (17 folders, 72+ examples)

### 5.5 Infrastructure ✅
- [x] GitHub Actions CI (check, clippy, fmt, test, build, validate examples) — path-filtered to skip docs-only changes
- [x] GitHub Actions Release workflow — builds Linux (musl), macOS (x86_64 + aarch64), Windows on version tags
- [x] Shared Lua sandbox module (`lua_sandbox.rs`) for consistent VM setup

### 5.6 Memory Hardening ✅
- [x] Bounded in-memory cache with LRU eviction + proactive TTL sweep (`src/util/bounded_cache.rs`)
- [x] `cache_set`/`cache_get` memory backend bounded by `IRONFLOW_CACHE_MAX_ENTRIES` (default 10 000)
- [x] MCP `INITIALIZED_SESSIONS` migrated to bounded cache with TTL — `IRONFLOW_MCP_SESSION_CACHE_SIZE` (default 1 024), `IRONFLOW_MCP_SESSION_TTL_SECS` (default 3 600)
- [x] OAuth token cache keyed by `(token_url, client_id, scope)`; bounded by `IRONFLOW_OAUTH_CACHE_SIZE` (default 128) — prevents cross-tenant token collision
- [x] Executor `step_map` shared via `Arc<HashMap<String, Arc<StepDefinition>>>`; per-task step and step_map clones removed
- [x] Task output persisted via direct `Value::Object` construction instead of `serde_json::to_value` round-trip
- [x] Node trait migrated to `&Context`; executor wraps context in `Arc<RwLock<Arc<Context>>>` with copy-on-write via `Arc::make_mut` — per-attempt deep clone eliminated
- [x] Subworkflow fan-out backpressure: `parallel_subworkflows` accepts `max_concurrent` (default: num_cpus, capped at 1 024); detached `subworkflow(wait=false)` bounded by process-wide semaphore `IRONFLOW_MAX_DETACHED_SUBWORKFLOWS` (default 64)
- [x] Run persistence: `RunSummary` type + `StateStore::list_run_summaries()` + `prune_before(cutoff)` trait methods; task outputs larger than `IRONFLOW_MAX_TASK_OUTPUT_BYTES` (default 2 MB) replaced with truncation marker
- [x] I/O size guards: `http_*`, `read_file`, `write_file`, `shell_command` all enforce configurable caps — `IRONFLOW_MAX_HTTP_BODY_BYTES` (50 MB), `IRONFLOW_MAX_FILE_BYTES` (50 MB), `IRONFLOW_MAX_SHELL_OUTPUT_BYTES` (10 MB, truncates with `_output_truncated` marker)

### 5.7 Correctness & Hardening ✅
- [x] `RunStatus::is_terminal()` helper; all three stores (`json`, `redis`, `null`) only stamp `finished` for terminal states
- [x] API flow-path resolver canonicalizes and confines paths under configured `flows_dir`; cwd fallback disabled in that mode; traversal and absolute-path escapes return HTTP 403
- [x] API `/runs` pagination: `?limit` (default 50, max 500), `?offset`; response includes `total`, `limit`, `offset`, `returned`

### 5.8 Streaming I/O & Native Summary Listings ✅
- [x] HTTP node: body streamed via `response.chunk()` with running byte counter; `Content-Length` pre-flight plus mid-stream overrun bail
- [x] Shell node: `wait_with_output()` replaced with concurrent bounded reads via `tokio::join!`; per-stream cap with drain-to-EOF to avoid pipe deadlock
- [x] MCP node: stdio bounded reads (same pattern as shell); SSE response consumed via `response.chunk()` with size guard
- [x] `JsonStateStore::list_run_summaries()` reads only `<id>.summary.json` sidecars — proven by test where corrupting the main record does not break the listing
- [x] `RedisStateStore::list_run_summaries()` fetches only the `summary` hash field, never the `info` blob
- [x] Regression tests: HTTP oversized-Content-Length rejection, shell output truncation marker, sidecar-based listing correctness under corrupt main record, pagination offset past the end yields empty page, status filter in summary listing
