# IronFlow delivered implementation baseline

This is the historical implementation baseline delivered through
`1.16.2-dev.4` on 2026-08-05. It records completed behavior and is not the
forward backlog. Current priorities and explicit product boundaries live in
the [`ROADMAP.md`](ROADMAP.md); executable findings and validation evidence live
in the [`issues/README.md`](issues/README.md) registry.

## Phase 1: Foundation ✅

The core engine, minimal node set, and CLI. Goal: execute a simple multi-step flow from a Lua file.

### 1.1 Project Scaffolding ✅
- [x] Set up Cargo workspace structure (edition 2024)
- [x] Add core dependencies: `tokio`, `mlua`, `serde`, `serde_json`, `clap`, `anyhow`, `thiserror`, `uuid`, `chrono`
- [x] Define module structure: `engine/`, `nodes/`, `lua/`, `storage/`, `cli/`, `api/`
- [x] Create `lib.rs` with public module exports

### 1.2 Context & Types ✅
- [x] Define `Context` type (`HashMap<String, serde_json::Value>`)
- [x] Define `RunStatus` enum: `Pending`, `Running`, `Success`, `Failed`, `Stalled`, `Cancelled`
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
- [x] Implement structured parallel executor using `FuturesUnordered` + `Semaphore`
- [x] Implement retry logic with exponential backoff
- [x] Phase-isolated context snapshots with declaration-order barrier commits
      and deterministic later-declared-writer collision precedence
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
- [x] Opt-in context variable interpolation with dotted keys, zero-based array
      indexes, JSON double-quoted bracket keys, validation, and literal escaping
- [x] `env(key)` function exposed to Lua for reading environment variables
- [x] `base64_encode(str)` / `base64_decode(str)` Lua globals (shared sandbox module)
- [x] Function handlers — pass Lua functions directly as step handlers (bytecode serialization)
- [x] `step_if(condition, name, handler)` — conditional step shorthand (syntactic sugar over `if_node` + `route`)

### 1.6 JSON State Store ✅
- [x] Implement `StateStore` trait
- [x] Implement `JsonStateStore` (file-based, `data/runs/{run_id}.json`)
- [x] Enforce the shared canonical 1–128 byte ASCII run-ID grammar at the JSON
  store boundary; reject invalid public `/runs/{id}` paths with `400`
- [x] Reject a symlinked/non-directory store root and symlinked/non-regular run
  or summary entries
- [x] Report historical JSON filenames with noncanonical run IDs as corruption
  instead of silently skipping them
- [x] Publish and replace each main record and summary atomically through a
  same-directory temporary file; keep the two files as independent commits
- [x] Link primary/summary payloads with an opaque revision and SHA-256 summary
  digest, treat the primary as authoritative, and derive plus best-effort
  repair missing, syntactically invalid, schema-unusable, or revision/digest-
  stale summary caches while rejecting an explicit foreign string sidecar ID
  as corruption.
  Document that the bounded summary fast path validates the header/projection
  while full reads and mutations validate the complete primary.
- [x] On Unix, enforce mode `0700` for the store directory and `0600` for main,
  summary, catalog base, delta, and other catalog metadata files; document the
  non-Unix ACL boundary
- [x] Maintain an immutable checksummed fixed-record summary-catalog base with
  global/per-status sections and a checksummed, coalesced 128-ID mutation
  delta. Version-2 dirty/clean state binds both revisions; one local writer
  lock, automatic recovery from authoritative primaries, bounded ordinary
  writes, periodic compaction, stopped-writer version transitions, and an
  explicit offline rebuild API preserve the projection lifecycle
- [x] Cover invalid/direct and percent-decoded API IDs, cross-instance
  no-clobber initialization, atomic replacement, filename/payload mismatch,
  non-regular entries, Unix symlinks, and new/legacy permissions
- [x] Thread-safe access via `tokio::sync::RwLock`

### 1.7 CLI ✅
- [x] `ironflow run <flow.lua>` — load, execute, print result
- [x] `ironflow validate <flow.lua>` — parse and check DAG, report errors
- [x] `--context` flag to pass initial context as JSON string
- [x] `--verbose` flag for detailed execution output (step details, task durations, outputs)
- [x] Pretty-printed output with task status indicators (✓, ✗, ⊘, ⟳, ○)
- [x] `ironflow list` — List bounded summary pages with `--status`, `--limit`,
  `--after`, and `--format`; JSON is a page envelope and there is no `--all`
- [x] `ironflow inspect <run_id>` — Show detailed run info as JSON
- [x] `ironflow nodes` — List available nodes with descriptions

### 1.8 Environment Configuration ✅
- [x] Atomically preload exactly cwd `.env` before tracing and CLI resolution;
      silently accept an absent default file
- [x] `--dotenv <path>` global CLI flag for a custom dotenv file; explicit
      missing, unreadable, and malformed files fail startup
- [x] Preserve existing process variables over dotenv values and expose
      dotenv-provided `RUST_LOG` to tracing
- [x] Environment variables accessible from Lua via `env(key)` function

---

## Phase 2: Nodes ✅

102 built-in nodes across HTTP, shell, file, S3, MCP, data transforms, conditionals, caching, database, AI, notifications, composition, S3 vector, XML, YAML, HTML sanitization, date/time, encoding, and utility categories. Each node is a Rust struct implementing the `Node` trait.

See [NODE_REFERENCE.md](NODE_REFERENCE.md) for the complete list with parameters, context output, and Lua examples.

---

## Phase 3: REST API & Persistence ✅

### 3.1 REST API Server ✅
- [x] `ironflow serve` command with `--host`, `--port`, `--flows-dir`, `--max-body` flags
- [x] `POST /flows/run` — Accept `source`, `source_base64`, or `file`, with initial context
- [x] `POST /flows/validate` — Validate without executing (node types, deps, DAG cycles)
- [x] `GET /runs` — List bounded summaries with optional `status`, `limit`, and
  filter-bound `after`; return `has_more` / `next_cursor` rather than offset or
  an exact total
- [x] `GET /runs/:id` — Get full run info (context, tasks, timing)
- [x] `DELETE /runs/:id` — Delete state and retained events, fence late event
  publication, and recover orphaned event cleanup on retry (404 when neither
  state nor orphaned events exist; 409 while a non-terminal owner lease is live)
- [x] `GET /nodes` — List registered nodes with descriptions
- [x] `GET /health` — Version and status check
- [x] Split process liveness from storage/admission readiness and drain on SIGTERM/SIGINT
- [x] `source_base64` field for escaping-free Lua submission
- [x] Mutual exclusion — reject requests with multiple source fields
- [x] Configurable request body size limit (default 1 MB, `--max-body` flag)
- [x] Handler error responses use safe JSON (`error` + stable `code`);
  `InvalidInput` maps to `400`, while internal errors omit details and return
  an opaque `error_id` mirrored in `X-Error-ID`
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
- [x] Immutable-incarnation plus opaque revision-token CAS with jittered retry/rebase for lossless concurrent run, task, and context mutations and delete/recreate fencing
- [x] Atomic, preflighted run initialization, deletion, and stale-index cleanup
- [x] Inspect up to 32 ordered-catalog members per steady-state run-list
  request through a persistent cursor with a per-cycle high-water boundary;
  revision-safely repair every valid live member and remove cold TTL leftovers
  without starvation from newer inserts. Protect the one-time legacy Set
  rebuild with a renewable owner lease and finalized generation recheck.
- [x] Backward-compatible first-write adoption for hashes created before incarnation/revision tokens, plus validated migration of non-aliasing unsafe raw keys
- [x] `create_store()` factory function for backend selection (config + env var)
- [x] `AppState.store` refactored to `Arc<dyn StateStore>` for runtime backend selection
- [x] Real-Redis integration coverage, including barrier-driven concurrent writers and wrong-type fault injection

### 3.3 SQL State Store ✅
- [x] `SqlStateStore` for SQLite/Postgres via `sqlx::AnyPool`
- [x] Separate SQL tables for runs and tasks to avoid rewriting full run records on task updates
- [x] Delete each run/task set transactionally and prune all selected terminal
  runs in one transaction; serialize task upserts through the same per-run
  mutation lock, propagate faults, and roll the operation back
- [x] Backend selection via `IRONFLOW_STORE=json|sqlite|postgres|redis`
- [x] SQL store URL via `IRONFLOW_STORE_URL` / `store_url`
- [x] SQL table prefix via `IRONFLOW_SQL_TABLE_PREFIX` / `sql_table_prefix` for shared SQLite/Postgres databases

### 3.4 Run Event Streaming ✅
- [x] Define compact `RunEvent` payloads for run/task lifecycle monitoring; include step name, `node_type`, attempt, status, timing, and error metadata, but never full node input/output.
- [x] Add separate event backend selection via `IRONFLOW_EVENT_STORE=memory|sqlite|postgres|redis` and `IRONFLOW_EVENT_STORE_URL`; do not reuse `IRONFLOW_STORE` so deployments can store runs and events in different systems.
- [x] Fail closed in explicit replica mode unless both state and event stores are shared and durable; provide Docker owner-death acceptance.
- [x] Implement a globally bounded in-memory event store for single-instance
  deployments; `event_memory_capacity` /
  `IRONFLOW_EVENT_MEMORY_CAPACITY` defaults to 10,000 retained events and
  deletion fences and rejects zero, while a fixed 64 MiB retained-heap estimate
  independently bounds variable-length metadata and deque allocation.
- [x] Implement SQL event replay for SQLite/Postgres with transactionally
  allocated monotonic per-run sequences, run-scoped `(run_id, id)` identity, a
  unique sequence index, a partial null-sequence probe index, and 256-row
  transactional repair of legacy rows. Verify the managed sequence index and
  reject negative/altered counters or no-progress repair. Guard the global-ID
  primary-key migration, including deferrable PostgreSQL keys, and require a
  coordinated stop/no direct downgrade for old writers.
- [x] Apply the shared SQL table prefix to SQL event tables.
- [x] Implement Redis event store behind the `redis` cargo feature flag, using `REDIS_URL`, `REDIS_PREFIX`, and optional `REDIS_TTL`.
- [x] Publish and read Redis event list/index/sequence state through preflighted
  Lua scripts; make identical event retries idempotent, atomically fence and
  unlink current-layout streams only after command preflight. Adopt legacy
  layouts on Redis 6.2+ by atomically moving eligible families into
  deterministic exact-run quarantine, reading at most 128 head elements and
  returning at most 1 MiB per batch, and persisting generation-bound pending
  intents around
  same-list `LMOVE` rotations. Require two matching rolling payload/index
  digest passes with atomic final acknowledgement, restore binary-invalid or
  otherwise invalid payloads in reverse rotations bounded by both count and
  bytes, and return a typed conflict after 32 confirmed steps when more work
  remains. Enforce the alias-safe/exact-owner boundary, immediate alignment to
  an absolute TTL deadline with terminal expiry cleanup, fail-closed
  orphan-snapshot recovery, non-creating capability probes, and manual handling
  for ambiguous unsafe families (IF-030).
- [x] Add idempotent counted event deletion to every backend. Delete payloads
  and install a late-publication fence atomically within the event backend;
  retain fences durably in SQL, by configured TTL/persistence in Redis, and
  within the bounded in-memory queue for memory.
- [x] Emit events from the workflow engine next to run/task state transitions.
- [x] Add the initial `GET /runs/{id}/events` SSE endpoint with
  `?after=<event_id>` replay from the selected event backend.
- [x] Drain complete event-store pages without dropping entries, preserve each
  backend's current stable read order, and continue from the last emitted ID as
  an exact exclusive cursor. SQL resolves opaque event IDs to its monotonic
  per-run publication sequence rather than ordering by timestamp plus UUID.
- [x] Resolve a non-empty `Last-Event-ID` header ahead of the bootstrap `after`
  query, preflight and retain the first page before sending headers, and return
  `410 event_cursor_gone` for unknown, wrong-run, or expired cursors.
- [x] Flush `run_finished` and close; also close an already-terminal replay
  after the available retained events have drained.
- [x] Replace swallowed polling and serialization failures with one safe,
  ID-less `stream_error` control event followed by EOF; mark only backend
  failures retryable, and retain one-second empty polling plus 15-second
  ID-less keep-alive comments.
- [x] Add shared Memory/SQLite/Postgres/Redis ordering and cursor-contract tests
  plus API tests for header precedence, multi-page replay, terminal EOF,
  pre-response errors, in-stream errors, and event-encoding failure.
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
- [x] Value-source-backed precedence: explicit CLI > existing process
      environment > selected dotenv > config file > built-in default; an
      explicit value equal to a default still wins
- [x] Webhook routes via config — scalar or structured `webhooks:` entries create `POST /webhooks/{name}` endpoints; request headers are default-denied, explicitly forwarded only through redacted execution overlays, and optional environment-backed HMAC-SHA256 policies verify the exact request body before run creation
- [x] Scheduled triggers — `schedules:` block in `ironflow.yaml`, evaluated by a
      30-second tick inside `ironflow serve`. Cross-replica at-most-one claims via
      `StateStore::claim_schedule` (SQL unique index, Redis `SET NX EX`, JSON
      exclusive create, Null process-local always-true). A successful claim can
      still be deliberately burned by lateness, bounded best-effort overlap,
      capacity, or a start failure. Claim keys are local wall-clock, so a
      fall-back hour is claimed once; a spring-forward gap fires after the gap.
      Schedule names evaluate concurrently under a 15-second budget, and the
      scheduler task is supervised with the API server so task death cannot
      leave a healthy HTTP process with dead triggers. Configuration is bounded
      to 256 names, finite string/context/grace sizes, and 64 catch-up instants
      per tick. Restricted day-of-month and weekday fields use traditional OR
      semantics. JSON and SQL retention is cadence-limited and deletes at most
      256 schedule-scoped claims per pass; JSON uses digest/hour shards while
      retaining its rolling-upgrade-compatible atomic file and incrementally
      indexing legacy claims, SQL uses a covering cleanup index, and Redis
      relies on per-key TTL.
- [x] Storage backend selection via config — `store_backend`, `store_url`,
  `event_store`, `event_store_url`, `event_memory_capacity`, and
  `sql_table_prefix`; env-var equivalents include `IRONFLOW_STORE`,
  `IRONFLOW_STORE_URL`, `IRONFLOW_EVENT_STORE`,
  `IRONFLOW_EVENT_STORE_URL`, `IRONFLOW_EVENT_MEMORY_CAPACITY`, and
  `IRONFLOW_SQL_TABLE_PREFIX`

### 5.3 Testing ✅
- [x] Unit tests for each node (in `test_nodes` and domain-specific suites)
- [x] Integration tests for the workflow engine
- [x] Lua flow parsing and sandbox tests
- [x] State store tests
- [x] API endpoint tests
- [x] Interpolation unit tests
- [x] Hundreds of tests across tens of test files

### 5.4 Documentation ✅
- [x] Node reference with individual per-node files (`docs/nodes/`)
- [x] Lua flow writing guide (`docs/LUA_FLOW_GUIDE.md`)
- [x] CLI and environment variable reference (`docs/CLI_REFERENCE.md`)
- [x] Examples organized by category with README

### 5.5 Infrastructure ✅
- [x] GitHub Actions CI (module-size ratchet, audit, default/full-feature
  Clippy and tests, fmt, Linux/macOS builds, validate examples) — runs on pushes
  to `develop` and `main` (plus explicit manual dispatch), with path filters
  that skip docs-only changes while checker and policy changes remain in scope.
  Routine work uses focused local checks; `.githooks/pre-push` runs the full
  integration gate before `develop`.
  Linux example validation reuses the release-build artifact without waiting
  for macOS or recompiling it. Container publication separates a cargo-chef
  dependency layer from source and package-version changes and exports it to a
  dedicated zstd-compressed GHCR BuildKit cache manifest. The mutable cache tag
  is never a deployment reference. Default Clippy/tests share one Linux job;
  full-feature Clippy and required Redis/PostgreSQL tests share another, so CI
  keeps backend coverage without isolated check or per-backend compilations.
  On `main`, CI also compiles the default and full Windows release dependency
  graphs into one dependency-only cache. Tag builds restore that default-branch
  cache read-only while compiling and packaging both binaries from the tag; a
  cache miss remains a correct cold build rather than a release failure.
- [x] Schema-v2 example catalog classifies all 132 Lua flows in this baseline,
  records composable service/credential/state/platform requirements, and
  evaluates every flow against the built-in registry so all 102 node types
  remain covered without exemptions
- [x] GitHub Actions Release workflow — builds Linux (musl), macOS (x86_64 + aarch64), and Windows on version tags; the parallel Windows variants reuse the exact `main` dependency graph without reusing a prebuilt application binary
- [x] Shared Lua sandbox module (`src/lua/sandbox.rs`) for consistent VM setup

### 5.6 Memory Hardening ✅
- [x] Immutable local `ArtifactRef` store keeps large binary handoffs out of
  workflow context; PPTX media, `read_file`, and PDF renderers publish
  content-addressed files while extractors and image/PDF consumers resolve
  descriptors without Base64
- [x] DOCX and PPTX XML parts parse directly from bounded ZIP-entry readers
  with cumulative decoded-byte accounting, incremental UTF-8 validation,
  cancellation checkpoints, and end-of-entry CRC verification
- [x] Bounded in-memory cache with LRU eviction + proactive TTL sweep (`src/util/bounded_cache.rs`)
- [x] Bound the default in-memory run-event backend globally to 10,000 events
  and deletion fences plus a fixed 64 MiB retained-heap estimate, with
  oldest-first eviction; expose a positive YAML/env count override
- [x] `cache_set`/`cache_get` memory backend bounded by `IRONFLOW_CACHE_MAX_ENTRIES` (default 10 000)
- [x] Stateful MCP sessions use opaque handles, capacity-bound least-recently-used eviction, and idle expiry — `IRONFLOW_MCP_SESSION_CACHE_SIZE` (default 1 024), `IRONFLOW_MCP_SESSION_TTL_SECS` (default 3 600); eviction and expiry close the owned transport
- [x] OAuth token cache keyed by `(token_url, client_id, scope)`; bounded by `IRONFLOW_OAUTH_CACHE_SIZE` (default 128) — prevents cross-tenant token collision
- [x] Executor `step_map` shared via `Arc<HashMap<String, Arc<StepDefinition>>>`; per-task step and step_map clones removed
- [x] Task output persisted via direct `Value::Object` construction instead of `serde_json::to_value` round-trip
- [x] Node trait migrated to `&Context`; executor wraps context in `Arc<RwLock<Arc<Context>>>` with copy-on-write via `Arc::make_mut` — per-attempt deep clone eliminated
- [x] Subworkflow fan-out backpressure: `parallel_subworkflows` accepts `max_concurrent` (default: num_cpus, capped at 1 024); detached `subworkflow(wait=false)` bounded by process-wide semaphore `IRONFLOW_MAX_DETACHED_SUBWORKFLOWS` (default 64)
- [x] Run persistence: `RunSummary` type + `StateStore::list_run_summaries()` + `prune_before(cutoff)` trait methods; task outputs larger than `IRONFLOW_MAX_TASK_OUTPUT_BYTES` (default 2 MB) replaced with truncation marker
- [x] I/O size guards: `http_*`, `read_file`, `write_file`, `shell_command` all enforce configurable caps — `IRONFLOW_MAX_HTTP_BODY_BYTES` (50 MB), `IRONFLOW_MAX_FILE_BYTES` (50 MB), `IRONFLOW_MAX_SHELL_OUTPUT_BYTES` (10 MB per stream, exposes optional `{output_key}_output_truncated = true`)

### 5.7 Correctness & Hardening ✅
- [x] `RunStatus::is_terminal()` helper; all state backends only stamp `finished` for terminal states
- [x] Supervised run coordinator with typed workflow/infrastructure outcomes, panic containment, detached-waiter safety, explicit cancellation, task-state repair, and terminal-write retry
- [x] Total step execution deadline shared across attempts/backoff; deadline-aware Lua blocking workers, structured child-run cancellation, shell process-tree cleanup, and cancellation-safe MCP session invalidation
- [x] API flow-path resolver canonicalizes and confines paths under configured
  `flows_dir`; cwd fallback is disabled in that mode, and existing/missing
  traversal or absolute-path escapes return the same generic HTTP 404
- [x] API `/runs` keyset pagination: `?limit` (default 50) and filter-bound
  `?after`; response includes `limit`, `returned`, `has_more`, and
  `next_cursor`. API and CLI share the positive `IRONFLOW_MAX_LIST_RECORDS`
  hard cap (default 100) and expose no unbounded listing mode.
- [x] Define stable ordering at UTC microsecond precision:
  `started DESC NULLS LAST, id DESC`; sub-microsecond timestamp differences
  deliberately fall through to the run-ID tie-breaker.
- [x] Record the cursor redesign as a next-major-version migration: HTTP
  `offset` and exact `total` are removed, CLI JSON changes from a full-record
  array to a summary page envelope, `StateStore` requires the page method, and
  `AppState` / `ServeOptions` require `ListingPolicy`.

### 5.8 Streaming I/O & Native Summary Listings ✅
- [x] HTTP node: body streamed via `response.chunk()` with running byte counter; `Content-Length` pre-flight plus mid-stream overrun bail
- [x] Shell node: `wait_with_output()` replaced with concurrent bounded reads via `tokio::join!`; per-stream cap with drain-to-EOF to avoid pipe deadlock
- [x] MCP node: persistent newline-framed stdio sessions bounded by `IRONFLOW_MAX_SHELL_OUTPUT_BYTES`; MCP 2025-11-25 Streamable HTTP supports correlated JSON and SSE responses without exposing transport session IDs
- [x] `JsonStateStore` reads the revision from a bounded primary prefix and
  uses `<id>.summary.json` only on an exact match; stale/missing/malformed
  caches fall back to the authoritative primary and are best-effort repaired
- [x] `RedisStateStore::list_run_summaries()` fetches only the `summary` hash field for current records, with an `info` fallback for pre-summary legacy hashes
- [x] Redis cursor pages use native global and per-status Sorted Set indexes,
  with lexicographically encoded microsecond/ID keys and
  `ZREVRANGEBYLEX LIMIT limit + 1`; the pre-index Set catalog is rebuilt lazily
  once, while run/status mutations, deletion, and stale-entry cleanup maintain
  the ordered indexes atomically. Derived keys use the non-run
  `run_catalog:v1` namespace, and cardinality checks rebuild independently
  missing indexes. A persistent maintenance cursor inspects up to 32
  independent catalog entries per steady-state page, uses a fixed cycle
  high-water boundary, and revision-safely repairs full index membership while
  sweeping cold TTL-expired records. Rebuilds use an owner lease and finalized
  generation marker so pages cannot observe partial generations.
- [x] SQLite/PostgreSQL cursor pages use native
  `{prefix}runs_started_idx` / `{prefix}runs_status_started_id_idx` ordering,
  fetch only `limit + 1`, and boundedly backfill the numeric microsecond key
  for legacy and mixed-version rows before paging.
- [x] JSON cursor pages binary-search an immutable checksummed fixed-record base,
  range-read at most `limit + 1 + K` entries from its global/status section,
  and merge a checksummed coalesced delta where `K <= 128`. Clean pages are
  O(log N + page size + K), use O(page size + K) memory, and do not enumerate
  the store directory. Ordinary projection writes are O(K); the 129th distinct
  overlay ID compacts O(N) records and resets the delta. Version-2 dirty/clean
  state, a local file lock, and authoritative-primary recovery cover malformed
  base/delta metadata (IF-029, IF-033).
- [x] Regression tests: HTTP oversized-Content-Length rejection, shell output
  truncation marker, revision/digest-linked sidecar repair and fault injection,
  bounded cursor traversal, status-bound cursors, SQL tie/null ordering,
  transactional SQL delete/prune rollback, CLI default/overridden caps, and
  invalid/over-limit rejection
- [x] Redis ordered-page regressions: a two-record page over 121 runs does not
  read a corrupt oldest summary, status transitions and deletion move/remove
  index entries, TTL-expired stale entries are cleaned during native paging,
  summary identity mismatches fail as corruption, missing indexes rebuild,
  historical raw run keys cannot alias catalog metadata, production-arity Lua
  failures leave no partial writes, legacy Set catalogs receive a lease-owned
  generation-safe backfill, and the 32-entry maintenance cursor uses a cycle
  high-water boundary to repair live index drift and reach expired records
  below repeatedly requested newest pages even during continuous inserts.
- [x] Redis atomicity regressions: paused barrier races for state/event writers,
  delete/recreate incarnation fencing, alias-safe keys, bounded and resumable
  legacy revision/layout adoption, deterministic quarantine, pending rotations,
  reverse restoration, idempotent and conflicting event retries, mixed-old-
  writer collisions, and preflight crash-point failures
- [x] Event lifecycle regressions: global memory eviction, idempotent counted
  deletion, SQL monotonic and concurrent publication, bounded SQL legacy
  repair, resumable Redis legacy adoption, SQL/Redis deletion races, and API
  retry cleanup of orphaned streams
