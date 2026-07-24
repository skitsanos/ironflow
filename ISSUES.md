# IronFlow Audit Backlog

This is the working backlog from the deep Rust, documentation, and Lua-example
audit performed on 2026-07-22. Each finding has a stable ID, evidence, a bounded
outcome, and an acceptance gate so issues can be fixed one at a time.

`docs/superpowers/` is historical planning material, not a current product
contract.

## Working agreement

1. Select one issue (or one tightly coupled pair) from the highest-priority open
   group and mark it `In progress`.
2. Add regression coverage that demonstrates the original defect.
3. Align implementation, current-contract docs, and examples.
4. Run the issue-specific checks and the complete repository gate.
5. Mark an issue `Resolved` only after all acceptance criteria pass; record the
   completion date and commit/PR when applicable.

Complete Rust gate:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo check --all-targets --features postgres,redis
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features postgres,redis -- -D warnings
```

Redis changes also require a dedicated disposable server. `CLIENT PAUSE` is
server-wide, so never point this gate at a shared or production Redis:

```bash
IRONFLOW_REDIS_TEST_URL=redis://127.0.0.1:<port>/ \
IRONFLOW_REDIS_TEST_REQUIRED=1 \
  cargo test --all-targets --features redis -- --test-threads=1
```

Complete static Lua gate:

```bash
find examples -type f -name '*.lua' -print0 |
  while IFS= read -r -d '' file; do
    ./target/debug/ironflow validate "$file"
  done
```

Static validation must be supplemented with representative runtime probes.

## Status summary

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
| IF-001 | P0 | Resolved | Lua security | Package loader restores removed OS/I/O libraries |
| IF-002 | P0 | Resolved | Runtime safety | Invalid numeric configuration panics or deadlocks |
| IF-003 | P0 | Resolved | Runtime safety | Lua-to-JSON cycles can abort the process |
| IF-004 | P0 | Resolved | Engine | Runs can remain `running` after internal failure/cancellation |
| IF-005 | P0 | Resolved | Storage | Redis state/event mutations are not atomic |
| IF-006 | P1 | Resolved | Engine | Step timeout is not a total/preemptive deadline |
| IF-007 | P1 | Resolved | Engine | `on_error` bypasses DAG semantics and can erase failure state |
| IF-008 | P1 | Resolved | Lua DSL | `step_if(...):depends_on(...)` evaluates its guard too early |
| IF-009 | P1 | Resolved | CLI | Failed workflows exit with status code 0 |
| IF-010 | P1 | Resolved | MCP | Stdio lifecycle/JSON-RPC validation do not support real servers |
| IF-011 | P1 | Resolved | API security | Webhooks persist credentials and cookies in run context |
| IF-012 | P1 | Resolved | API events | SSE drops batches, hides errors, and never terminates |
| IF-013 | P1 | Resolved | API security | Internal errors and connection URLs can leak secrets |
| IF-014 | P1 | Resolved | Storage security | JSON run IDs/permissions need hardening |
| IF-015 | P1 | Resolved | Text processing | Fixed AI chunking can corrupt UTF-8 |
| IF-016 | P1 | Resolved | Examples | Clean checkout lacks fixtures used by 40 examples |
| IF-017 | P2 | Resolved | API/CLI | Documented loopback hosts do not all bind |
| IF-018 | P2 | Resolved | CLI config | Dotenv/config precedence is inconsistent |
| IF-019 | P2 | Resolved | Engine | Parallel context-key collisions are nondeterministic |
| IF-020 | P2 | Resolved | Storage | Summaries, deletes, and event retention can drift |
| IF-021 | P2 | Resolved | API/storage | Run pagination loads the full catalog |
| IF-022 | P2 | Resolved | Nodes | Shell failure discards documented structured output |
| IF-023 | P2 | Resolved | Interpolation | Examples use unsupported expressions/array paths |
| IF-024 | P2 | Resolved | Docs | Node counts, backends, APIs, and signatures have drifted |
| IF-025 | P2 | Resolved | Quickstart | README commands/API payloads are not runnable as written |
| IF-026 | P2 | Resolved | Examples | Examples overwrite inputs or retain machine-specific state |
| IF-027 | P2 | Resolved | Features | PostgreSQL disabled-feature errors are unclear |
| IF-028 | P3 | Resolved | Architecture | Large/duplicated modules need bounded extraction |
| IF-029 | P3 | Resolved | JSON storage | Run pages required full filesystem catalog scans |
| IF-030 | P2 | Resolved | Redis events | Legacy event adoption/deletion is bounded and resumable |
| IF-031 | P2 | Resolved | S3 Vectors | Examples cannot delete indexes or vector buckets |
| IF-032 | P2 | Resolved | S3 Vectors | Resource targets can mix explicit and environment identifiers |
| IF-033 | P3 | Resolved | JSON storage | Projection-changing writes replace the complete run catalog |
| IF-034 | P3 | Resolved | Architecture | Module-size policy has no automated regression guard |

## P0 — release-blocking safety and durability

### IF-001 — Lua package-loader sandbox escape

**Status:** Resolved on 2026-07-22.

`Lua::new()` loads mlua's `ALL_SAFE`, including `os`, `io`, and `package`.
Clearing global `os`/`io` left them in `package.loaded`, so `require("os")`
restored command execution during flow validation. Flow loading, `code`, and
`foreach` used the vulnerable VM profile.

Implemented locally:

- Lua VMs start from an allowlist (`table`, `string`, `math`, `utf8`).
- Package, OS, and I/O libraries are never loaded.
- Dynamic source/bytecode loaders, caller-controlled GC, and `string.dump` are
  unavailable.
- One VM factory is used by flow loading, code nodes, and foreach transforms.
- Loader/code/foreach capability tests and safe-library compatibility tests
  were added.

Acceptance: the forbidden capabilities remain unavailable in every VM, all Lua
examples validate, and Lua runtime/limit/code/foreach tests pass.

### IF-002 — Invalid numeric configuration panics or deadlocks

**Status:** Resolved on 2026-07-22.

Evidence:

- Negative/NaN/oversized seconds reached `Duration::from_secs_f64`, panicking a
  Tokio worker instead of returning a node error.
- `:timeout(-1)` left persisted run/task state as `running` after the panic.
- `IRONFLOW_MAX_CONCURRENT_TASKS=0` creates `Semaphore::new(0)` and waits
  forever.
- Floating-point conversion accepted `2^64` as `u64::MAX`; file-cache expiry
  then overflowed `now + ttl`.
- Retry count/backoff arithmetic can overflow or become non-finite.

Required outcome: central finite/range-checked duration conversion at every
user-controlled site, rejection of present-but-invalid values, checked integer
and retry arithmetic, no zero-concurrency deadlock, and boundary regression
tests.

Implemented locally:

- All user-controlled durations use fallible finite/range-checked conversion.
- Flow validation rejects invalid timeout/retry values, duplicate step names,
  and retry-count overflow before a run is initialized.
- Zero engine concurrency is normalized safely; node-level zero concurrency is
  rejected explicitly.
- Narrow integer conversions, cache expiry, SMTP ports, image quality, batch and
  chunk sizes, PDF pages, and semantic-window arithmetic are checked.
- Boundary regressions cover negative/non-finite durations, retry overflow,
  cache TTL overflow, invalid sizes, and zero concurrency.

### IF-003 — Unbounded/cyclic Lua-to-JSON conversion

**Status:** Resolved on 2026-07-22.

Both `src/lua/runtime/conversion.rs` and `src/nodes/utility/code.rs` recursively
convert tables without cycle/depth tracking. A self-referential table can reach
Rust stack overflow/process abort. Mixed tables can lose keys, unsupported Lua
values silently become `null`, and empty `{}` cannot explicitly mean `[]`.

Required outcome: one bounded converter with active-cycle detection, depth/node
budgets, path-rich rejection of mixed/unsupported values, explicit empty
array/object semantics, and cyclic/deep/mixed/shared-table tests.

Implemented locally:

- One VM-aware converter now owns loader, extractor, code, foreach, logging, and
  JSON-helper conversion.
- Active-path table identity tracking rejects cycles while allowing a shared
  non-cyclic table to appear in multiple branches.
- Fixed 64-level and 100,000-node budgets bound traversal and diagnostics name
  the failing JSON-style path.
- Sparse/mixed tables, unsupported values, non-finite numbers, and invalid keys
  fail explicitly instead of becoming `null` or losing data.
- `json_array`, `json_object`, and `json_null` preserve empty shapes and nulls;
  JSON parse/context round trips retain them.
- A binary child-process regression proves a cyclic result becomes an ordinary
  failed run rather than aborting the test process. All 125 examples still
  validate under the stricter converter.

### IF-004 — No guaranteed run terminalization

**Status:** Resolved on 2026-07-22.

After a run becomes `Running`, a later `?`, task panic, `JoinError`, dropped
execution future, or store failure can exit without setting a terminal status
or `finished`. Spawned work is not centrally drained/aborted; `Stalled` is never
assigned; the `completed` set is write-only.

Required outcome: a run coordinator/guard owning joins and best-effort
terminalization, explicit workflow/infrastructure/cancellation states, defined
shutdown behavior, and panic/store-error/cancellation tests.

Implemented locally:

- `WorkflowEngine::start()` returns a supervised `RunHandle`; dropping a waiter
  detaches without abandoning execution, while `RunHandle::cancel()` persists
  explicit `Cancelled` run/task states.
- Phase work uses structured concurrent futures instead of detached Tokio task
  handles. A panic or infrastructure failure drops every sibling future before
  terminal state is written, preventing late task mutations.
- Task execution distinguishes controlled workflow/node failures (`Failed`)
  from persistence, executor, and panic failures (`Stalled`); infrastructure
  failures no longer enter normal `on_error` routing.
- One finalizer persists partial context, repairs every initialized
  `Pending`/`Running` task, retries the terminal run write, and emits one
  best-effort `RunFinished` event only after durable terminalization.
- Skipped and cancelled tasks now carry `finished`; terminal runs enforce that
  every initialized task is terminal.
- Dedicated regression coverage injects task-state, final-context, and
  terminal-status failures; panics beside blocked parallel work; explicit
  cancellation; and dropped waiters.

Contract boundary: this guarantees in-process convergence while the state
backend can eventually accept writes. Permanent backend failure and hard
process/host termination cannot execute an async finalizer; crash recovery
requires a future ownership-lease/reconciliation design.

### IF-005 — Non-atomic Redis state and event writes

**Status:** Resolved on 2026-07-22.

Redis state mutations read and rewrite a full `RunInfo` without CAS, so parallel
task updates can overwrite each other. Event publish performs `INCR`, `RPUSH`,
and `HSET` separately, allowing cursor/list-order disagreement and partial
writes. Existing Redis tests are sequential.

Required outcome: independent task records or revisioned WATCH/MULTI/Lua CAS;
atomic event publish or Redis Streams; barrier-driven concurrency and
crash-point tests against disposable Redis.

Implemented locally:

- Run hashes retain their compatible `info`/`summary` representation and add
  an immutable incarnation plus an opaque revision token. Every status, task,
  and context mutation reads one snapshot, rebases after a same-incarnation
  conflict, and commits record, summary, next revision, and sliding TTL in one
  Lua CAS. Conflicts use capped exponential full-jitter backoff without an
  arbitrary attempt ceiling; an incarnation mismatch fences stale work after
  delete/recreate.
- Initialization preflights both run and index key types before writing either;
  deletion preflights the index before atomically deleting the run and removing
  its index member. Update scripts require an existing run, so a writer arriving
  after deletion cannot recreate it. Legacy hashes without tokens remain
  readable and gain deterministic incarnation plus revision fields on their
  first successful mutation.
- UUID-like historical key segments remain unchanged. Unsafe/reserved IDs use
  an injective encoded namespace. Valid non-aliasing raw legacy state keys
  migrate atomically after embedded run-ID checks. Redis event candidates use
  IF-030's bounded protocol only when the physical family is alias-safe or
  carries the exact owner marker; ambiguous ownerless unsafe current families
  require manual migration, while non-owned optional raw candidates remain
  untouched and are ignored.
- Redis event publication preflights all four key types and the legacy
  list/index/sequence invariant before atomically recording the event, cursor,
  sequence, layout marker, and TTLs. Repeating the same event is idempotent for
  contents and sequence while refreshing configured retention; reusing its ID
  for a different payload fails. Legacy adoption atomically moves the eligible
  family into deterministic exact-run quarantine before validation. Persisted
  generations and pending intents surround bounded same-list `LMOVE`
  rotations; two matching rolling payload/index digest passes receive the
  owner marker, while validation failures reverse-rotate and restore the
  original family. Replay then reads cursor and list in one script snapshot.
- Redis tests now accept `IRONFLOW_REDIS_TEST_URL`; an explicitly configured or
  required server cannot silently skip. Test prefixes are UUID-isolated and
  cleanup only their own keys.
- Paused, barrier-released races cover 24 task plus 24 context writers racing
  terminalization, 320 contending state writers, and 24 unique event
  publishers. Exact-script regressions cover duplicate publication,
  delete/recreate incarnation fencing, stale-sweep reinitialization, reserved
  and encoded-key aliasing, unsafe-key migration, TTL limits, bounded and
  resumable legacy adoption, historical raw/encoded namespace collisions,
  conflicting/empty event IDs, and wrong-type/malformed-sequence crash points.

Acceptance boundary: these scripts provide atomic isolation for each Redis
state or event operation. They do not combine state persistence and event
publication into one transaction, do not add Redis Cluster support, and do not
replace Redis persistence configuration for host-crash durability. Redis TTLs
are limited to `99,999,999,999` seconds so every script conversion remains
exact and preflightable. Rolling upgrades must not mix old non-CAS writers with
the new layout.

## P1 — correctness, security, and public-contract defects

### IF-006 — Timeout contract is neither total nor fully preemptive

**Status:** Resolved on 2026-07-22.

`tokio::time::timeout` cannot preempt synchronous `code`/`foreach` Lua work that
does not yield. It wraps each retry attempt while backoff occurs outside the
deadline, so `retries(3):timeout(30)` can take about 120 seconds plus backoff,
despite README “total step timeout” wording. Blocking side effects may continue
after timeout.

Decide per-attempt versus total-step semantics, move CPU/blocking work off
runtime workers, connect Lua deadlines/cancellation, document side-effect
cancellation limits, and test loops/retries/shell process trees/blocking nodes.

Implemented locally:

- `timeout()` is one deadline created before the first attempt. The same
  deadline encloses every `node.execute()` call and exponential retry backoff;
  it never resets, late-ready results are rejected, and a retry event is only
  emitted after its backoff completes within budget. Durable task state records
  the stable `Task '<name>' timed out after <seconds>s total` error.
- A task-local execution deadline plus drop-cancellation flag bridges async
  execution to synchronous workers. `code`, `foreach`, and nested-flow Lua
  loading run on Tokio's blocking pool; mlua instruction hooks and Rust phase
  checkpoints stop pure-Lua loops and conversions without monopolizing runtime
  workers.
- Structured `subworkflow`, `parallel_subworkflows`, and `tool_dispatch` child
  runs use cancel-on-drop waits. Parallel children are owned by a `JoinSet`, and
  spawned tasks explicitly inherit the parent deadline. The documented
  `subworkflow(wait = false)` mode remains intentionally detached.
- Shell and MCP stdio share an RAII child-process guard. Dropping the node
  future terminates the direct child on every platform; Unix children lead an
  inherited process group that is killed for node timeout, step timeout, and
  workflow cancellation.
- Cancellation remains cooperative. Completed external effects cannot be
  rolled back; native/blocking callbacks run until their next checkpoint;
  non-Unix descendant trees lack a job-object equivalent; and Unix descendants
  that explicitly escape their process group can outlive the step. Lifecycle
  persistence is not forcibly interrupted by the execution deadline.
- Fourteen focused regressions cover total retry/backoff budgets, successful
  in-budget retry, cancellation during backoff, dropped async futures,
  current-thread Tokio responsiveness, infinite `code`/`foreach`/child-loader
  Lua, shell and MCP process trees, and cancellation of all three structured
  composition paths.

### IF-007 — `on_error` is not a valid recovery DAG

**Status:** Resolved on 2026-07-22.

Handlers execute immediately and ignore declared dependencies. Handler failure
can later be overwritten as `Skipped`; self/mutual references can skip all work
and succeed; shared handlers race global `_error_*`; normal steps referenced as
handlers are removed from ordinary scheduling.

Validate missing/self/cyclic/shared targets and unsupported metadata, represent
recovery as planned work with dependencies, preserve attempts/failures, and add
dependency/failure/cycle/concurrency tests.

Implemented locally: one shared execution planner now validates and schedules
normal dependencies, source-to-handler activation, and handler-to-downstream
recovery barriers. Validation rejects missing, self-referencing, shared,
nested, routed, source-dependent, and cyclic handlers; handler dependencies,
retries, and total timeouts remain supported.

Handlers run only in their planned phase. Successful recovery clears the
source from unresolved scheduling failures while retaining the source task's
durable `Failed` record; failed handlers retain their own final attempt and
`Failed` state. `_error_message`, `_error_step`, and `_error_node_type` are
invocation-local overlays, so concurrent handlers cannot race or leak metadata
into shared context. Seven focused regressions cover validation, augmented
cycles, dependency ordering, downstream barriers, handler failure/retry state,
untriggered recovery branches, and concurrent metadata isolation. All default
and `postgres,redis` tests and Clippy gates pass; all 125 Lua examples validate,
and the updated recovery example completes successfully end to end.

### IF-008 — `step_if` evaluates its guard before dependencies

**Status:** Resolved on 2026-07-22.

The hidden guard starts with no dependencies. Chained `:depends_on(...)` is
added only to the visible step, so a condition can evaluate before prerequisite
context exists and permanently select the wrong route.

Attach user dependencies to the guard, keep the visible step dependent on the
guard, extract the duplicated `step`/`step_if` builder, and add an execution
test using upstream context.

Implemented locally: the builder retains the hidden guard and forwards chained
dependencies to it, producing prerequisite -> guard -> visible action. Lua
extraction and true/false execution tests verify dependency and timestamp
ordering. The later IF-028 modularity pass completed the builder deduplication.

### IF-009 — Failed CLI workflow exits zero

**Status:** Resolved on 2026-07-22.

`ironflow run` prints `Status: failed` but returns `Ok(())`, contradicting
`docs/CLI_REFERENCE.md` and making CI/CD status checks unsafe. Return nonzero for
`Failed`, define `Stalled`, preserve printed run information, and add a real
binary-process test.

Implemented locally: every persisted non-success terminal status is reported in
full and then returned as a CLI error, so the process exits nonzero. Binary-level
tests cover successful and failed runs. A full `Stalled` lifecycle remains part
of IF-004.

### IF-010 — MCP transport/session contract fails with real stdio servers

**Status:** Resolved on 2026-07-22.

Each stdio action starts a process, keeps stdin open, and waits for exit before
accepting the response. Long-lived MCP servers do not exit after one message,
and session state cannot survive initialize/initialized/list-tools across new
processes. The one-shot mock hides this. Responses are accepted without
checking JSON-RPC version or matching `id`; SSE naming/behavior needs alignment
with a pinned current MCP transport spec.

Implement persistent framed stdio sessions, strict response correlation and
shutdown, a specification-aligned HTTP transport, and tests with a real
long-lived server.

Implemented locally:

- The client is pinned to stable MCP `2025-11-25` and uses the official Rust
  SDK for the initialization lifecycle, correlated requests, server messages,
  cancellation, and Streamable HTTP session behavior. `initialize` now
  completes the handshake atomically; the obsolete public `initialized` action
  and standalone `sse` transport name are rejected with migration guidance.
- One opaque IronFlow handle owns each persistent stdio or Streamable HTTP
  session. Later `list_tools`, `call_tool`, and `close` actions reuse it. Raw
  server `MCP-Session-Id` values and JSON-RPC envelopes never enter workflow
  context; capacity, idle expiry, failure, cancellation, and explicit close all
  release transport ownership.
- IronFlow supplies a strict stdio transport around the SDK: compact UTF-8 JSON
  messages are newline framed and bounded by
  `IRONFLOW_MAX_SHELL_OUTPUT_BYTES`; malformed/ambiguous envelopes and invalid
  versions/IDs close the session rather than being skipped. Writes are
  acknowledged only after they reach the child pipe.
- Stdio shutdown closes stdin, waits, requests termination, then escalates to
  the existing Unix process-group kill guard. Dropping an in-flight node future
  invalidates the session synchronously and preserves IF-006 descendant cleanup.
- Streamable HTTP owns its content negotiation, negotiated protocol and server
  session headers. JSON and incremental SSE responses are supported, a matching
  SSE response completes without waiting for EOF, one 404 expiry is
  reinitialized/replayed, and close performs the protocol DELETE cleanup.
- The example Python server is now a real long-lived lifecycle fixture. Both
  Lua examples use opaque sessions and explicit close; the former `mcp_sse.lua`
  example is replaced by `mcp_streamable_http.lua`. Current-contract docs and
  cross-references describe the new inputs, outputs, lifecycle, limits, and
  security boundary.

Acceptance evidence:

- Long-lived stdio tests prove one PID handles
  initialize/initialized/list/call/EOF, identical configurations remain
  isolated, unrelated IDs are not accepted, server notifications and ping are
  dispatched separately, malformed envelopes and unsupported negotiation fail,
  and timeout/future-drop cleanup terminates the process tree.
- Streamable HTTP tests prove JSON and open-ended SSE response handling,
  transport-owned headers, private server sessions, initialized `202`
  semantics, 404 reinitialization/replay, and DELETE close.
- `cargo test --all-targets` and
  `cargo test --all-targets --features postgres,redis` pass, as do both required
  Clippy commands with warnings denied, all 125 Lua examples validate, and the
  documented stdio workflow completes successfully end to end.

### IF-011 — Webhook credentials are persisted as context

**Status:** Resolved on 2026-07-22.

Every UTF-8 request header is copied into `_headers` before initial state is
persisted, including platform authorization/API keys, cookies, proxy auth,
cloud/session tokens, signatures, and PII. Docs/examples intentionally read
authorization there, so a blind filter is a breaking change.

Separate platform auth from workflow ingress, introduce an execution-only
secret/header overlay that stores/events redact, default-deny credential
headers, support explicit business-signature forwarding, and test backing-store
and GET-run redaction.

Resolution:

- Webhook configuration now accepts the existing scalar flow path or a strict
  structured form with explicit `forward_headers`. No headers are forwarded by
  default; platform authorization, cookies, proxy credentials, and common
  cloud/session credential headers are reserved and rejected in configuration.
- API authentication consumes `Authorization` and `X-API-Key` before protected
  handlers run. Configured business headers are normalized, must be unique
  text values of at least eight non-whitespace bytes, and are exposed only in
  the invocation-local `ctx._headers` overlay. Request bodies cannot spoof
  `_headers`, `_webhook`, or `_flow_dir`.
- `WorkflowEngine` now owns a task-local execution overlay and redaction policy.
  Initial context, task output/error, shared context, final context, events,
  child workflows, and coordinator panic messages cross their boundaries only
  after overlay keys and literal/structured secret forms are scrubbed. The
  policy is explicitly inherited by subworkflow, parallel-subworkflow, and
  tool-dispatch child engines.
- `GET /runs/{id}`, `ironflow inspect`, and JSON run listings defensively use
  credential-like values from historical `_headers` maps to redact legacy
  records without hiding ordinary fields in non-webhook runs. Operators are
  documented to purge old state and rotate previously exposed credentials.
- Webhook/config suites exercise the real API-key middleware, legacy and strict
  config forms, default-deny behavior, explicit signature access, ambiguous and
  invalid input rejection, reserved context keys, raw JSON state, every state
  write argument, event errors, public run output, recovery, and nested
  subworkflow propagation. The Lua example now validates a distinct business
  signature in place instead of persisting an Authorization bearer token.

### IF-012 — SSE loses batches, hides errors, and never terminates

**Status:** Resolved on 2026-07-23.

The handler fetches 100 events, emits one, discards the rest, and re-queries.
Store errors become empty batches/infinite waits; polling continues after
`RunFinished`; invalid cursors replay from the start; store failures can map to
404.

Buffer/drain full batches, surface backend errors with bounded policy, terminate
after terminal events, support `Last-Event-ID`, define expired cursor behavior,
and test every event backend.

Resolution:

- The SSE adapter retains and drains every event in each bounded 100-event
  page before advancing its exclusive cursor. Full pages trigger the next read
  immediately, while an empty non-terminal stream keeps the one-second poll
  interval and 15-second ID-less keep-alive comments.
- A non-empty UTF-8 `Last-Event-ID` header now overrides the bootstrap `after`
  query, matching native `EventSource` reconnection behavior. Empty headers
  fall back to the query; duplicate fields and NUL/CR/LF cursors are rejected.
  Unknown, wrong-run, and expired initial cursors return `410 Gone` with
  `event_cursor_gone` instead of silently replaying from the beginning.
- Initial event-store reads complete before the SSE response is committed, so
  typed backend and corruption failures retain the normal safe HTTP contract.
  After `200`, a storage or encoding failure emits exactly one safe, ID-less
  `stream_error` and closes without advancing the durable cursor. Only backend
  failures are marked retryable; permanent corruption, conflict, cursor loss,
  and serialization failures are not.
- `run_finished` is flushed once and then closes the stream. Persisted terminal
  run state is the fallback source of truth when the best-effort terminal event
  is missing: retained pages drain, one delayed empty grace read covers the
  status/event publication race, and the stream then reaches EOF.
- Memory, SQL, and Redis now share exact empty/zero-limit/cross-run cursor
  semantics. SQL validates the full cursor row before resolving its durable
  per-run sequence; Redis performs zero-limit cursor validation atomically in
  Lua. IF-020 subsequently replaced SQL's former wall-clock/UUID ordering with
  transactionally allocated monotonic publication positions.
- Production delivery code is split into a 113-line HTTP/preflight module and
  a 248-line stream protocol module. API and backend contract suites are split
  into similarly focused test/support modules, and README, architecture, CLI,
  and implementation-plan documentation describe the implemented behavior.

Acceptance evidence:

- `cargo test --all-targets`, `cargo fmt --all -- --check`, and both strict
  default and `postgres,redis` Clippy matrices pass with warnings denied.
- The dedicated SSE contract suite passes 11 tests, including 205-event
  multi-page replay, UTF-8 reconnect precedence, terminal EOF, HTTP `410`,
  preflight `500`, bounded in-stream failures, and unsafe stored identities.
- The live feature matrix passes 8 EventStore contract tests against configured
  PostgreSQL and a disposable `redis:latest` instance, 20 Redis atomicity tests,
  and the 11 SSE tests. The Redis container is removed after validation.

### IF-013 — API/store errors can disclose connection credentials

**Status:** Resolved on 2026-07-22.

Internal responses serialize full chained errors; Redis/Postgres constructors
and logs can include full URLs with userinfo. Some handlers instead map every
store error to 404, hiding outages.

Return generic 500 JSON with an error ID, redact URL userinfo in logs/errors,
and introduce typed not-found/backend/corruption/conflict storage errors.

Resolution:

- `StateStore` and `EventStore` now return the concrete `StorageResult` type.
  Every JSON, null, SQL, Redis, and memory implementation classifies failures
  as `NotFound`, `Backend`, `Corruption`, or `Conflict`; pruning ignores only a
  genuine concurrent disappearance, and duplicate event identities or missing
  cursors no longer masquerade as successful reads.
- API handlers preserve safe `404`/`409` outcomes while backend and corruption
  failures return a generic `500` object. Each internal response carries a
  unique UUID in both `error_id` and `X-Error-ID`; the server records one
  sanitized diagnostic with the same ID, and no internal `details` or chained
  driver error reaches the client. Delete uses the typed result directly
  instead of a racy read preflight.
- Connection diagnostics use a fail-closed URL sanitizer that removes user
  information, redacts every query value, drops fragments, and hides paths for
  secret endpoints such as webhooks. Storage errors retain no raw driver source,
  known database, Redis, ArangoDB, SMTP, Slack, and HTTP call sites use redacted
  display wrappers, and the executor scrubs node error text before logging or
  persistence.
- The SQL state implementation was split into focused store, codec, and schema
  modules. Regression tests cover exact category mapping, generic correlated
  API failures, duplicate/cursor/corrupt records, malformed and encoded URLs,
  and credential sentinels across HTTP, database, ArangoDB, email, and Slack
  diagnostics.
- A 2026-07-23 feature-matrix follow-up found that SQLx can detach an arbitrary
  query value from a PostgreSQL URL and repeat it in a configuration error.
  Database connection failures now expose only the already-redacted endpoint,
  never the third-party driver cause.

Acceptance evidence:

- `cargo test --all-targets`, `cargo fmt --all -- --check`, and both strict
  default and `postgres,redis` Clippy matrices pass with warnings denied.
- A live configured PostgreSQL instance passes all 33 focused state/event tests.
- A disposable `redis:latest` container passes all 61 focused Redis,
  state/event, and atomicity tests; the container is removed after validation.

### IF-014 — JSON run path and permissions are not hardened

**Status:** Resolved on 2026-07-23.

JSON store paths interpolate caller-provided run IDs; public `{id}` accepts
arbitrary strings; run files containing secrets inherit process umask.

Validate canonical IDs at API/store boundaries, prevent root escape/symlink
traversal, atomically create/replace owner-only run/summary files, and test
traversal/encoding/symlink/permissions.

Resolution:

- Public run routes and the JSON store now share one byte-exact run-ID
  validator: 1–128 ASCII bytes, alphanumeric boundaries, and only
  alphanumeric, `-`, or `_` bytes. Invalid storage input has its own typed
  category and public run endpoints reject it with `400 bad_request` before a
  store call; generated UUIDv4 IDs remain a valid subset.
- The JSON backend was split into focused store, codec, filesystem, listing,
  platform, and temporary-file modules. It rejects symlinked/non-directory
  roots and symlinked/non-regular managed entries, uses no-follow opens on
  Unix, validates filename/payload identity, and reports historical
  noncanonical managed names as corruption.
- New main records use synced, uniquely named `create_new` temporary files
  (`0600` on Unix) and atomic hard-link publication without clobbering an
  existing run. Replacements use synced same-directory temporary files and
  rename; abandoned/cancelled temporary opens retain a cleanup guard.
- Unix directories and run/summary files are enforced to `0700` and `0600`,
  including tightening legacy files on access. Non-Unix ACLs and final
  link/rename guarantees remain filesystem/operator responsibilities.
- Regression coverage exercises direct and percent-decoded traversal IDs,
  every public run endpoint, cross-instance initialization races, concurrent
  raw readers during replacement, temporary cleanup on cancellation,
  filename/payload mismatches, non-regular entries, symlinks, and Unix modes.

Acceptance evidence: `cargo test --all-targets`, both default and
`postgres,redis` `cargo check --all-targets`, both strict Clippy matrices with
warnings denied, formatting/diff checks, and static validation of all 125 Lua
examples pass. The main record and derived summary remain independent
filesystem commits; the summary commit is best effort, but IF-020 now links
them by revision, treats the primary as authoritative, and repairs stale
derived summaries. IF-029 subsequently added the durable ordered JSON catalog.

### IF-015 — Fixed AI chunking can corrupt UTF-8

**Status:** Resolved on 2026-07-22.

Chunking slices byte windows at arbitrary offsets and repairs fragments with
`from_utf8_lossy`; multibyte characters can become replacement characters and
delimiter matching is byte-oriented. Tests are ASCII-only.

Keep fixed-mode byte budgets while using UTF-8 boundaries, treat delimiters as
characters and `min_chars` as characters, and verify concatenated chunks equal
the exact input.

Implemented locally: fixed boundaries never split a Unicode scalar, oversized
scalars remain whole, delimiter matching is character-aware, and `min_chars`
counts characters. Multilingual, emoji, delimiter, exact-roundtrip, and zero-size
regressions pass. Extended grapheme-cluster boundaries remain a possible future
semantic enhancement.

### IF-016 — Versioned example fixtures are missing

**Status:** Resolved on 2026-07-23.

All 125 Lua files statically validate, but 40 tracked examples reference ignored
`data/...` assets; a clean checkout tracks none. Fourteen reference one absent
PDF, with other missing PDF/VTT/PPTX/image inputs. Static validation does not
execute nodes.

Add small licensed fixtures under `examples/fixtures/` or a versioned download
manifest with checksums, use example-relative paths, and run a clean-checkout
offline matrix while classifying external-service examples separately.

Resolution:

- Added a 64 KiB synthetic CC0 fixture pack with reviewed SHA-256 checksums:
  PDF, DOCX, PPTX, PNG, VTT, and SRT. The document fixtures were rendered and
  visually inspected; the PPTX also passed the slide overflow validator.
- Replaced every tracked `data/samples` dependency with stable flow-relative
  fixture paths where runtime interpolation supports them. Generated outputs
  are kept outside the fixture pack, destructive shell preparation uses unique
  run-owned temporary directories, and external shell examples document their
  repository-root requirement. Broader output cleanup remains tracked by
  IF-026.
- Removed nine fixture-missing early returns from extraction/PDF tests. The
  real fixtures now exercise PDF, DOCX, PPTX, VTT, SRT, image, metadata,
  chunking, and MCP stdio behavior on a clean checkout.
- Added an exhaustive catalog that classifies all 125 Lua flows exactly once as
  offline, output/process, network, credentialed external, server/manual, or
  composition cases. CI rejects missing, duplicated, or unclassified flows.
- Added an isolated runtime gate for ten fixture-backed workflows plus the
  persistent MCP stdio example. It runs outside the repository working
  directory, verifies successful CLI completion, and keeps native Pdfium,
  macOS Quick Look, credentials, remote mutations, and network calls explicitly
  capability-gated.

Acceptance evidence: all 125 Lua flows pass static validation; the fixture and
catalog suite passes 4/4; extraction passes 21/21; PDF/image passes 10/10; both
default and `postgres,redis` all-target suites pass; doc tests, both strict
Clippy matrices, formatting, and diff checks pass.

## P2 — behavior, operability, and consistency

### IF-017 — `localhost` and IPv6 loopback do not bind

**Status:** Resolved on 2026-07-22.

Authentication recognizes `127.0.0.1`, `localhost`, and `::1`, but server bind
parses `format!("{host}:{port}")` directly as `SocketAddr`; DNS names and raw
IPv6 fail. Bind through Tokio tuple/`ToSocketAddrs`, base loopback policy on
resolved addresses, and test IPv4/hostname/IPv6.

Implemented locally: Tokio now resolves and binds `(host, port)` directly; the
actual bound address controls the loopback authentication exception. Unit tests
bind successfully through `localhost`, IPv4 loopback, and raw IPv6 loopback.

### IF-018 — Dotenv/config precedence is inconsistent

**Status:** Resolved on 2026-07-23.

Clap resolves `env=` before `.env`/`--dotenv` is loaded; tracing reads
`RUST_LOG` before dotenv; explicit missing dotenv only warns; explicit CLI
values equal to defaults may fail to override config because precedence is
inferred by value comparison. Use two-stage dotenv preload and real value-source
resolution: explicit CLI > environment > config > default.

Resolution:

- Startup now performs a synchronous bootstrap pass before Tokio worker
  threads exist, atomically parses exactly the selected dotenv file, preserves
  pre-existing process variables, and initializes tracing from the merged
  environment before the final Clap parse. Auto-discovery is limited to
  `./.env`; only an absent automatic file is silently accepted.
- Dotenv read/parse failures are fatal and secret-safe. The whole file is
  validated before any environment mutation, the first duplicate declaration
  wins, and malformed source lines or values are never included in diagnostics.
- Clap `ValueSource` now distinguishes explicit CLI/environment values from
  built-in defaults. The deterministic contract is explicit CLI > existing
  process environment > selected dotenv > `ironflow.yaml` > built-in default,
  including when an explicit value equals its default. `IRONFLOW_STORE_DIR`
  applies consistently to `run`, `list`, `inspect`, and `serve`.
- `IRONFLOW_MAX_CONCURRENT_TASKS`, API authentication settings, storage
  settings, and Redis TTL now use strict environment parsing ahead of YAML;
  invalid higher-priority values fail instead of silently falling through.
- Tracing writes to stderr so diagnostics cannot corrupt machine-readable CLI
  stdout such as `list --format json`.
- README, CLI/architecture/Lua documentation, the sample YAML, example guide,
  environment-variable Lua example, and a sanitized `.env.example` now state
  and demonstrate the same contract.

Verification covers source selection at every precedence tier, explicit
default-valued flags, exact dotenv discovery, atomic/secret-safe failures,
pre-tracing `RUST_LOG`, strict typed environment values, Redis TTL handling,
and the clean-checkout Lua example matrix. Default and all-feature test suites,
formatting, and warnings-as-errors Clippy gates pass.

### IF-019 — Parallel context merges are nondeterministic

**Status:** Resolved on 2026-07-23.

Parallel tasks merge outputs as they finish. Same-key writes and phase/event
order depend on scheduling/hash iteration. Define a collision policy
(validation error, namespacing, or deterministic plan-order merge), sort ready
phases, and add repeated stress tests.

Resolution:

- Lua flow extraction and topological ready phases now preserve source
  declaration order instead of relying on Lua-table or hash-map iteration.
- Every task and retry in a DAG phase receives one immutable phase-start
  context snapshot. Successful output and terminal structured-failure output
  are reduced into a phase-local per-key winner accumulator until the phase
  settles, avoiding full-output retention for values that already lost a
  collision.
- The barrier commits output in declaration order. A later-declared
  same-phase step wins duplicate keys and emits a value-free collision warning;
  a later dependency phase can still overwrite earlier context intentionally.
- Task lifecycle events retain real execution timing. Per-task history remains
  task-local subject to `IRONFLOW_MAX_TASK_OUTPUT_BYTES`, and `_error_output`
  remains source-exact even when a different task wins the shared key.
- Cancellation or infrastructure failure before the barrier publishes none of
  that phase's buffered context, preventing a timing-dependent partial result.
- README, architecture, Lua/contributor guides, SSE and node contracts, and
  affected Lua examples now describe and follow the same semantics.

Verification covers reversed completion timing over 25 repeated runs,
concurrency-one and retry isolation, dependent-phase overwrites,
structured-failure recovery collisions, phase-atomic cancellation and
infrastructure failure, source declaration order, all Rust targets, every Lua
example, and strict Clippy.

### IF-020 — Storage summaries/deletes/retention can drift

**Status:** Resolved on 2026-07-23.

JSON sidecar failures are ignored while stale sidecars are trusted; SQL deletes
tasks/runs without one transaction; `prune_before` ignores delete failures; run
deletion leaves events; memory events are unbounded; SQL event order relies on
wall-clock timestamp plus UUID. Redis run TTLs can also leave Set/hash/Sorted
Set catalog entries behind indefinitely when expired runs remain below the
pages users traverse. Add revisioned/transactional summaries,
transactional lifecycle operations, event retention/deletion, monotonic DB
cursors, bounded Redis catalog maintenance, and failure tests.

Resolution:

- JSON primary and summary payloads now share a generated opaque revision and
  SHA-256 digest of the serialized public summary. The primary remains
  authoritative: listing trusts the compact sidecar only when a bounded
  primary-header read matches both values and the sidecar recomputes to the
  digest. Missing, syntactically invalid, schema-unusable, or revision/digest-
  mismatched sidecars fall back to a full primary decode and best-effort repair;
  an explicit string sidecar ID that disagrees with its filename is corruption
  and does not fall back. Legacy
  unversioned/revision-only
  primaries use the full-record path without a repeated unusable repair until
  their next mutation upgrades them. The bounded fast path intentionally does
  not decode the unused primary suffix; full reads and mutations validate the
  complete primary. A post-primary summary commit or repair failure is logged
  without converting a durable mutation into a false failure, and deletion
  retries recover either half of interrupted main/sidecar cleanup.
- SQL single-run deletion removes tasks and the run row in one transaction.
  SQL `prune_before` locks eligible terminal runs and deletes every selected
  run/task set in one transaction, rolling all changes back on a fault. Task
  upserts use the same per-run mutation lock, preventing an orphan insert
  behind delete/prune. Both paths verify that no run or task row remains before
  commit, so trigger-suppressed or compensating mutations fail as corruption
  and roll back. The default trait implementation now ignores only a concurrent
  `NotFound` and propagates every other deletion failure.
- `EventStore` now requires idempotent, counted per-run deletion. Each backend
  removes retained payloads and installs a publication fence within its own
  atomicity boundary. `DELETE /runs/{id}` uses a shared lifecycle coordinator:
  state is deleted first, and a retry after event-store failure can remove the
  orphaned stream even though state is already absent. Missing state plus no
  orphaned events remains `404`; embedded callers can use the same coordinator
  instead of the state-only trait method.
- The memory event backend is a single oldest-first queue across all runs,
  including deletion fences. `event_memory_capacity` or
  `IRONFLOW_EVENT_MEMORY_CAPACITY` selects a positive count and defaults to
  10,000; a fixed 64 MiB retained-heap estimate independently accounts for
  variable-length strings and deque allocation. Either limit evicts oldest
  entries, individually oversized events are rejected, and evicted cursors
  correctly become unavailable.
- SQL events now receive transactionally allocated monotonic per-run
  publication sequences, enforced by a unique `(run_id, sequence)` index.
  Opaque event identity is `(run_id, id)`, allowing two runs to reuse an ID.
  Existing nullable rows are adopted in stable legacy `(timestamp, id)` order
  through 256-row transaction batches; a partial null-sequence index keeps the
  frequent remaining-legacy probe bounded. The global-ID primary-key migration
  locks and validates the schema: SQLite refuses unsupported columns, indexes,
  triggers, or foreign keys, while PostgreSQL rejects extra uniqueness/
  exclusion and deferrable primary keys, and rolls back dependency failures.
  Both dialects verify that the managed sequence index is the exact live unique
  `(run_id, sequence)` index, so an occupied/spoofed name cannot silently remove
  cursor uniqueness. It requires a coordinated stop of old writers and is not
  directly downgrade-safe once cross-run IDs are reused. Allocation rejects
  negative or trigger-altered counters before publication. A legacy backfill
  verifies every guarded update/delete and rejects a no-progress trigger or
  altered result instead of retrying forever. Publish/delete share the same
  per-run lock, and deletion verifies the durable fence, payload absence, and
  counter absence before commit, so partial cleanup cannot be reported as
  success or recreate an orphan stream.
- Every steady-state Redis run-summary page now claims up to 32 catalog members
  through a persistent maintenance cursor independent of the public cursor.
  A per-cycle high-water boundary prevents continuous newer inserts from
  starving wraparound; every valid live member receives revision-safe full
  global/status-index repair and expired members are removed. Revision
  conflicts defer to a later cycle rather than spinning on a hot run. Missing
  or inconsistent derived catalogs rebuild from the legacy Set behind a
  renewable owner lease and finalized generation marker, which pages recheck
  before returning. Redis current-layout event deletion preflights `UNLINK`,
  then atomically fences and removes its list/index/sequence/layout namespace.
  IF-030 replaces the remaining unmarked-stream full snapshot with bounded,
  resumable, two-pass `LMOVE` validation in deterministic exact-run quarantine
  plus bounded reverse restoration.
- Fence lifetime is explicit: SQL tombstones are durable until operator
  cleanup; Redis fences persist without `REDIS_TTL` and otherwise share that
  TTL; memory fences share the configured queue and disappear on restart or
  eviction. Direct `StateStore::delete_run` and `prune_before` remain
  state-only low-level operations; the API uses the cross-store lifecycle
  coordinator.

Acceptance evidence: 14 JSON revision/digest tests cover stale, missing,
malformed, tampered, revision-only, repair-failure, interrupted-deletion, and
bounded/noncanonical-prefix cases; SQL rollback, suppressed-mutation,
no-progress backfill, and task/delete/prune races cover both SQLite and
PostgreSQL; memory tests cover count/byte eviction and oversized events; API
tests cover coordinated deletion and orphan retry. On 2026-07-23,
`cargo test` and the serial
`cargo test --all-features -- --test-threads=1` matrix passed, including
doctests. A disposable `redis:latest` server (Redis 8.8.0) passed all 33 Redis
atomicity and 15 Redis state-store tests, including generation-safe rebuild,
continuous-insert maintenance, balanced status repair, and denied-`UNLINK`
failure atomicity plus single-command TTL fence installation. A
PostgreSQL-enabled matrix against disposable PostgreSQL
18.4 passed 16 event-store tests, 13 guarded identity/migration/backfill tests,
and both SQL state-race tests. Both all-target checks,
`cargo clippy --all-targets -- -D warnings`, its
`--all-features` counterpart, all 125 Lua validations, formatting, and diff
checks passed.

### IF-021 — Pagination occurs after full-catalog loading

**Status:** Resolved on 2026-07-23.

Before resolution, the API loaded all summaries, sorted them, and only then
applied offset/limit; SQL `fetch_all`, Redis `SMEMBERS`, and JSON scans were
O(N). The goal was to move ordering/filter/limit/cursor into the store query
contract and test large-catalog plans/memory.

Resolution:

- `StateStore` now requires a bounded `list_run_summaries_page` primitive with
  a validated non-zero page size, optional status filter, and filter-bound
  opaque keyset cursor. Stable ordering uses start time at UTC microsecond
  precision, descending with missing timestamps last, then run ID descending;
  sub-microsecond differences intentionally fall through to the ID tie-breaker.
- `GET /runs` and `ironflow list` exclusively use summary pages. Offset and
  unbounded/all listing are not exposed; responses identify whether another
  page exists and provide its cursor. Exact `total` is removed, and CLI JSON no
  longer emits a top-level array containing full contexts and task histories.
- `IRONFLOW_MAX_LIST_RECORDS` is a positive hard cap shared by API and CLI and
  defaults to 100. It is resolved after dotenv loading; invalid/zero values
  fail rather than disabling the bound. API requests above the cap return 400.
- This is an intentional next-major-version migration boundary. HTTP clients
  must replace `offset` with `after` and stop expecting `total`; CLI JSON
  consumers must read the page envelope; external `StateStore` implementors
  must add the required page method; and embedded users must initialize the new
  `ListingPolicy` fields on `AppState` and `ServeOptions`. The development
  checkout still declares package version 1.12.0; release engineering must
  bump the major version before publishing these contracts.
- SQL applies filter, cursor, deterministic order, correlated task counts, and
  `LIMIT + 1` in the database, backed by `{prefix}runs_started_idx` and
  `{prefix}runs_status_started_id_idx`; bounded backfill covers both legacy
  startup rows and later mixed-version inserts.
- Redis pages use native global and per-status Sorted Set indexes. Their
  lexicographic members encode the normalized microsecond timestamp and run ID,
  and `ZREVRANGEBYLEX` fetches `limit + 1`; a one-time lazy migration examines
  the legacy Set catalog, after which atomic lifecycle scripts maintain the
  ordered indexes. Derived metadata uses a separate `run_catalog:v1` namespace,
  and constant-cost Set/hash/Sorted-Set cardinality checks rebuild a missing
  derived index rather than silently returning an incomplete page.
- At IF-021 closure, JSON still examined every filesystem catalog entry while
  retaining only `limit + 1` best summaries. IF-029 subsequently replaced that
  scan with durable ordered JSON indexing.
- Regressions cover cursor traversal, filter binding, timestamp ties and NULLs,
  API rejection behavior, the 100-record CLI default, environment overrides,
  and summary-only JSON output. Redis integration coverage proves that a
  two-record page over 121 runs does not touch a corrupt oldest summary, status
  transitions and deletion maintain the per-status/global indexes, TTL-expired
  entries are cleaned during native paging, summary ID mismatches fail as
  corruption, missing derived indexes rebuild, a historical raw run ID cannot
  alias the catalog namespace, production-arity Lua faults are preflighted, and
  a legacy Set catalog is backfilled once. IF-020 adds lease-owned generation
  rebuilds and a persistent 32-entry maintenance cursor with a cycle high-water
  boundary, so live index drift and cold TTL leftovers are eventually repaired
  even under continuous inserts and repeatedly requested newest pages.

Current Redis verification on 2026-07-23 used `redis:latest` (Redis 8.8.0): all
33 atomicity and 15 state-store tests passed, with Redis event coverage also
included in the serial all-feature matrix.

### IF-022 — Shell failure discards documented structured output

**Status:** Resolved on 2026-07-23.

The shell node builds stdout/stderr/code/success and then returns `Err` for a
nonzero exit, so no output reaches context. `_output_truncated` is implemented
but undocumented. Define `fail_on_nonzero` or structured errors preserving
diagnostics; align docs/tests.

Resolution:

- The public `Node` trait remains source-compatible. A focused `NodeFailure`
  type now carries an actionable message plus `NodeOutput`; its `Display` and
  custom `Debug` never format attached values. The same seam fixes
  `validate_schema` and `json_validate`, whose documentation also promised
  structured validation results on failure.
- The executor centralizes output redaction, JSON conversion, and
  `IRONFLOW_MAX_TASK_OUTPUT_BYTES` handling. Structured output remains private
  during retries, a successful retry publishes only its success output, and
  only the final completed failed attempt is merged into context and stored in
  the failed task. A timeout during backoff retains the last completed
  attempt; execution timeouts and ordinary operational errors have no stale
  structured output.
- Recovery handlers receive the normal final context keys and an exact,
  invocation-local `_error_output` object. This keeps recovery deterministic
  when concurrent tasks reuse a prefix. Webhook execution-overlay keys and
  values are redacted before task state, shared context, recovery input,
  events, or public persistence.
- `shell_command.fail_on_nonzero` is a strict, interpolation-aware boolean that
  defaults to `true`. The default preserves task failure/retry behavior while
  retaining terminal stdout, stderr, code, success, and truncation state;
  `false` makes a completed nonzero exit an inspectable successful node result
  without hiding spawn, I/O, cancellation, or timeout failures.
- The shell node page now documents the exact exit-policy matrix, per-stream
  cap, lossy UTF-8 conversion, optional truncation marker, retry and recovery
  behavior, and persistence boundary. Architecture, Lua recovery, contributor,
  validation-node, implementation, README, and example documentation now use
  the same contract. The shell example runs an explicit nonzero status probe.

Acceptance evidence:

- Direct and executor regressions cover strict/interpolated policy parsing,
  default and opt-out exits, custom output keys, both-stream truncation,
  terminal task/context persistence, retry isolation, successful retries,
  exhausted retries, backoff timeout, recovery `_error_output`, validation
  failures, and webhook-secret redaction.
- `examples/06-shell/run_commands.lua` validates and completes end to end with
  exit code 7 represented as a successful status-inspection task. All 125 Lua
  examples pass static validation.
- Default and `postgres,redis` `cargo test --all-targets` suites, both feature
  check matrices, both strict all-target Clippy matrices with warnings denied,
  formatting, and diff checks pass.

### IF-023 — Interpolation grammar and examples disagree

**Status:** Resolved on 2026-07-23.

Runtime supports simple dot paths only. Examples use expressions such as
`or env(...)` and arrays such as `${ctx.results[1].key}`, resolving to empty
strings. README says interpolation works “everywhere,” though nodes opt in.

Define one grammar and array-index convention, centralize recursive
interpolation, add a validator/linter, and replace fallback expressions with
explicit workflow steps.

Resolution:

- `${ctx...}` is now a strict navigation grammar shared by validation and
  rendering: dotted ASCII identifiers, zero-based JSON array indexes, and
  JSON double-quoted bracket keys can be combined. Operators, calls,
  wildcards, and fallback expressions are rejected. `${HOME}`-style foreign
  forms stay literal, `\${ctx.key}` is the runtime literal escape, and
  `$${ctx.amount}` remains a currency prefix followed by interpolation.
- Rendering is streaming and one-pass. Missing paths, type mismatches,
  out-of-range indexes, and `null` retain the compatible empty-string result;
  strings insert directly and other JSON values use compact JSON text. The
  parser, renderer, recursive value walker, and focused tests are split into
  modules of 111–178 lines.
- `FlowDefinition::validate_dag()` now recursively lints every parsed config
  string value, never object keys or raw Lua source, and reports the step plus
  exact config path. CLI/API validation and engine startup therefore use the
  same grammar, while library execution rejects invalid interpolation before
  initializing durable run state.
- The common `interpolate_value` walker replaced six copies in HTTP, MCP,
  ArangoDB, email, Slack, and LLM code. `shell_command` now explicitly
  interpolates `cmd`, every argument, `cwd`, and environment values while
  rejecting non-string entries instead of silently dropping them.
- Six S3 Vector examples migrated all 16 array references to zero-based
  indexes. PDF metadata computes its non-secret default in an explicit
  function step. The MCP Streamable HTTP example reads its optional token
  while constructing the flow so the credential is not persisted in workflow
  context. Current README, architecture, Lua/node guides, node pages, and
  example documentation define the same opt-in field behavior and grammar;
  invalid wildcard-style context placeholder prose was removed.

Acceptance evidence:

- Parser/unit regressions pass 11/11; interpolation validation passes 7/7;
  the shell/Markdown suite passes 14/14, including nested and indexed command,
  argument, working-directory, environment, and foreign-shell expansion cases.
- All 125 evaluated Lua flows pass `ironflow validate`; the fixture-backed PDF
  metadata flow completes end to end and returns the versioned fixture author.
- Default and `postgres,redis` `cargo test --all-targets` suites, doc tests,
  both strict all-target Clippy matrices with warnings denied, formatting, and
  diff checks pass.

### IF-024 — Current documentation has broad contract drift

**Status:** Resolved on 2026-07-22.

- Registry and node pages contain 98 nodes; README/architecture/Lua guide/node
  reference/implementation plan claim 100.
- README calls Redis/Postgres planned although both exist behind features.
- Architecture shows old `Node::execute` and incomplete `StateStore` signatures.
- CLI reference omits `-C/--config` and numerous limit/cache variables.
- Endpoint docs omit run-event SSE and underdescribe pagination.
- “No runtime dependencies” conflicts with native Pdfium requirements.
- `docs/KNOWN_ISSUES.md` says there are no known issues.

Generate inventories where practical and label implementation plans as history.

Updated locally: current docs consistently report 98 registered nodes/pages,
describe implemented feature-gated stores, match current traits/CLI flags/API
pagination and SSE routes, enumerate runtime limit variables, and state Pdfium's
native requirement. Historical planning material remains untouched.

### IF-025 — README quickstart is not runnable as written

**Status:** Resolved on 2026-07-22.

It builds `target/release/ironflow` but invokes bare `ironflow`; uses invalid
`--context '{...}'`; starts default `0.0.0.0` without the required API key; and
shows truncated Lua/base64 payloads. Use the built path, loopback host, complete
valid payloads, and exercise quickstart commands in CI.

Updated locally: quickstarts use the built binary, valid context input, loopback
API startup, and complete inline/base64 submissions. The documented hello/data
pipeline, validation, and both live API submission paths were smoke-tested.

### IF-026 — Examples hide inputs or retain machine state

**Status:** Resolved on 2026-07-23.

Several examples overwrite caller context with hard-coded valid data, making
documented failure invocations ineffective. SQLite uses fixed
`/tmp/ironflow_test.db`; a PPTX example silently requires macOS `qlmanage`;
`s3_list_buckets`, `s3vector_get_bucket`, and `s3vector_get_index` lack examples.
Make inputs explicit, use disposable unique state, label platform/external
requirements, and enforce registry-to-example coverage.

Resolution:

- Example defaults now apply only when a caller value is absent. Present empty,
  false, malformed, or wrong-shaped values remain visible to the real node and
  fail naturally instead of being replaced with valid demo data. Direct code
  step tests and complete CLI runs protect both the defaulting and documented
  failure paths.
- Local file, cache, image, PDF, ZIP, and SQLite examples use UUID-scoped paths
  below the first non-empty `TMPDIR`, `TMP`, or `TEMP`, falling back to the
  working directory. Each flow documents whether it cleans or retains output.
  Parallel SQLite runs use independent databases, report exactly two rows, and
  leave no database, WAL, or shared-memory sidecars after success.
- S3 examples use UUID-scoped local paths and object prefixes, state their
  credential/permission requirements, and document that success-dependent
  cleanup cannot roll back failed or interrupted runs. The bucket-list example
  verifies account-level listing access before any mutation. S3 Vector examples
  use collision-resistant resource names and explicitly disclose that vectors
  may be cleaned while buckets and indexes remain billable remote resources.
- `examples/catalog.json` schema 2 classifies all 125 flows exactly once and
  records composable external-service, credential, local-state, and platform
  labels plus concrete POSIX, Python, Pdfium, Poppler, macOS Quick Look, and
  repository-working-directory capabilities.
- Real Lua evaluation validates every graph and node reference against
  `NodeRegistry::with_builtins()`. All 98 registered node types are represented
  with no exemptions; the three previously missing nodes now appear in runnable
  S3 and S3 Vector workflows. Contributor documentation defines the coverage
  rule and the reasoned-exemption format.
- CI discovers Lua files recursively and runs separate catalog, caller-contract,
  and fixture/runtime suites. It rejects missing/duplicate catalog entries,
  inconsistent labels/capabilities, unknown nodes, invalid DAGs, stale or blank
  exemptions, ignored sample paths, and shared machine-wide temporary paths.

Acceptance evidence:

- All 125 Lua flows pass recursive `ironflow validate`.
- The focused example gate passes 8 tests: three catalog/registry tests, three
  caller/state contract tests, and two checksum/runtime-matrix tests.
- `cargo fmt --all -- --check`, both all-target `cargo check` commands, both
  all-target Clippy commands with `-D warnings`, and both default and
  `postgres,redis` all-target test suites pass.

### IF-027 — Disabled PostgreSQL feature fails unclearly

**Status:** Resolved on 2026-07-22.

Redis has explicit feature guidance, but a default build accepts PostgreSQL
configuration and later fails through sqlx. Gate state/event Postgres branches,
return “rebuild with `--features postgres`,” and add feature-matrix tests.

Implemented locally: state and event Postgres selection now checks the feature
before reading URLs or touching a driver and returns an explicit rebuild command.
Default-build tests cover both branches; the feature build covers the enabled
gate.

### IF-030 — Redis legacy event migration is unbounded

**Status:** Resolved on 2026-07-23.

Before this fix, owner-marked Redis event streams published, paged, and deleted
without materializing the complete stream, but adopting or deleting an
unmarked legacy stream used `LRANGE 0 -1`. IronFlow deserialized every retained
payload in one operation to prove ownership and a digest, so a large legacy
stream could monopolize Redis and process memory.

Resolution:

- The compatibility path requires Redis 6.2 or newer for `LMOVE`. Its fixed
  internal policy reads at most 128 list elements and returns at most 1 MiB of
  serialized payload to Rust per batch. One operation confirms at most 32
  bounded steps; unfinished validation or restoration returns typed `Conflict`
  with explicit retry guidance.
- Automatic migration is limited to alias-safe physical families and unsafe-ID
  families carrying the exact requested owner. An owner-marked encoded family
  is already current; an exactly owned optional raw family can migrate into
  that encoded namespace. An ambiguous ownerless unsafe current family requires
  manual migration, while a non-owned optional raw candidate remains untouched
  and is ignored.
- Before reading any payload, one Lua operation persists exact-run state with a
  token, generation, phase, cursor, sequence, policy, source presence, rolling
  digest, and absolute TTL deadline, then atomically renames the eligible family
  into deterministic exact-run quarantine. If that snapshot exists without its
  state, IronFlow fails closed and preserves it for manual recovery.
- Every batch is read from the quarantined list head with `LINDEX` and fully
  deserialized in Rust. The commit records a new generation and pending count,
  batch digest, next cursor, and rolling digest before same-list `LMOVE` rotates
  the head batch to the tail. A later transition verifies the pending tail
  before advancing the durable cursor, making disconnect recovery explicit.
- Two complete rolling payload/index-digest passes validate the family. Full
  rotations restore original list order. Only matching passes with no recreated
  source or destination family receive the owner marker and atomically return
  to the current namespace. The last verification acknowledgement restarts or
  finalizes in the same script; repeated digest drift eventually blocks.
- A validation fault enters bounded reverse restoration. Confirmed tail batches
  rotate back to the head in windows bounded by 128 elements and 1 MiB, with
  generation-bound pending intents, before the family is renamed to its original
  namespace and corruption is returned. Binary-safe replies ensure invalid UTF-8
  reaches Rust validation and restoration. Redis has no element-length metadata,
  so one oversized element must be read once; it is then rejected and the family
  is restored rather than adopted or deleted.
- Migration never refreshes retention. The initial shortest TTL becomes an
  absolute Redis-time deadline applied to every quarantined component. Renames
  and rotations preserve countdown; restoration and finalization can only align
  keys to that deadline or a shorter remaining TTL. Complete quarantine extended
  past the deadline is expired, and a later access removes its progress record
  after the governed namespaces disappear; partial families remain fail-closed.
  Capability probes use absent keys and never create probe records.
- Pre-protocol event writers require a coordinated stop. Recreation or mutation
  of the source or destination while the deterministic snapshot exists blocks
  migration and preserves every live family. Two matching digest passes are
  required, but quarantine keys are internal protocol state and must not be
  edited directly. Invalid data is restored or blocked; IronFlow performs no
  automatic delta merge or deletion of a live collision.
- Current-layout atomic publication, replay, deletion fencing, `UNLINK`
  preflight, and same-owner fence-lifetime semantics from IF-020 are unchanged.
  Legacy deletion reaches that path only after the bounded protocol proves and
  owner-marks the family.

Acceptance evidence: a disposable `redis:latest` container running Redis 8.8.0
passed all 53 Redis atomicity tests and all 15 Redis state-store tests. Focused
coverage includes 10,000-event count bounds; forward and reverse aggregate byte
bounds; generation/pending-intent resume; both digest passes and atomic final
acknowledgement; exact invalid-UTF-8 restoration; missing-state snapshots;
malformed, cross-run, and cursor faults at batch boundaries; raw/encoded
ownership ambiguity; unequal, extended, and expired TTL deadlines; the one-read
oversized-element boundary; non-creating command probes; mixed-writer
collisions; and publish/delete races. On 2026-07-23, `cargo test`, serial
`cargo test --all-features -- --test-threads=1`, both all-target checks,
`cargo fmt --all -- --check`, `git diff --check`, the required
`cargo clippy --all-targets -- -D warnings`, and the all-feature strict-Clippy
variant all passed.

### IF-031 — S3 Vector resources cannot complete their lifecycle

**Status:** Resolved on 2026-07-23.

IronFlow can create and inspect S3 Vector buckets and indexes, and can delete
individual vectors, but it cannot delete an index or its containing vector
bucket. Six examples therefore leave UUID-scoped billable resources behind
even after successful vector cleanup.

Resolution:

- Added `s3vector_delete_index` and `s3vector_delete_bucket` in a bounded
  lifecycle module, with target-resolution tests split into a sibling module.
- Destructive resource identifiers must be explicit node configuration, may
  interpolate context, never fall back to `S3_BUCKET` or `S3VECTOR_*`, and
  reject ambiguous name-plus-ARN targets before loading the AWS client.
- Index-name deletion requires an explicit bucket name. Provider request errors
  retain node-specific context, while successful outputs echo only the
  validated target and a success flag.
- The six disposable S3 Vector examples now report results, delete vectors,
  delete the index, and delete the bucket through one ordered dependency chain.
  The transcript indexer remains intentionally persistent and documents its
  manual teardown order. Current documentation covers both new nodes, common
  region/endpoint configuration, provider permissions and constraints, and the
  registry count of 100 nodes.

Acceptance evidence: all six disposable workflows passed against the AWS and
OpenAI configuration in `.env`; all 18 lifecycle deletion steps succeeded, and
an independent AWS lookup confirmed every UUID-scoped bucket was absent after
each run. Credentials were never printed. All 125 Lua flows validate, catalog
evaluation covers all 100 registered nodes without exemptions, 11 focused S3
Vector integration tests and seven lifecycle unit tests pass, and both default
and `postgres,redis` all-target checks, test suites, and strict Clippy gates
pass. `cargo fmt --all -- --check` and `git diff --check` also pass.

### IF-032 — S3 Vector resource targets can mix identifier sources

**Status:** Resolved on 2026-07-24.

The older `s3vector_get_bucket`, `s3vector_create_index`,
`s3vector_get_index`, `s3vector_put_vectors`, `s3vector_query_vectors`, and
`s3vector_delete_vectors` nodes resolve bucket name, bucket ARN, index name,
and index ARN independently. An explicitly configured ARN can therefore be
accompanied or shadowed by an environment-derived name, and some requests can
set mutually exclusive AWS fields. Centralize source-aware resource targeting
so explicit configuration wins, each operation sends exactly one
provider-supported identifier form, ambiguous explicit targets fail before
network access, and the destructive vector-deletion path receives an
explicit-target safety decision.

Resolution: S3 Vector identifiers now pass through one typed, source-aware
target resolver after context interpolation. The presence of any relevant
configured target field selects configuration for the whole target, so
environment identifiers cannot complete or override a partial explicit form.
Non-string, blank, conflicting-alias, incomplete, ambiguous, and
provider-unsupported combinations fail before client construction. With no
configured target, non-destructive nodes accept one coherent environment-only
shape; `S3VECTOR_BUCKET_NAME` takes precedence over the legacy `S3_BUCKET`
fallback. Vector, index, and bucket deletion all require explicit configured
targets.

Every S3 Vector operation now prepares a generated AWS SDK input before
building the client and sends that exact input with `send_with`. Request-shape
tests assert the supported name or ARN form and that every alternate identifier
field is absent. The resolver uses injected environment readers in tests, so
precedence and ambiguity coverage does not mutate process-global state. The new
target modules remain below 300 LOC.

Validation covers 48 focused S3 Vector unit tests, 11 node integration tests,
all 125 Lua examples, and a live `.env`-backed 11-step AWS lifecycle that
created, inspected, populated, queried, and then deleted its UUID-scoped
vectors, index, and bucket. Default and `postgres,redis` all-target checks,
complete test suites, and strict Clippy gates pass, together with doc tests,
`cargo fmt --all -- --check`, and `git diff --check`.

## P3 — maintainability and modularity

### IF-028 — Large/duplicated modules need bounded extraction

**Status:** Resolved on 2026-07-24.

At audit time, 26 Rust source files exceeded 300 LOC and ten exceeded 400 LOC.
High-value candidates:

- `src/nodes/ai/llm_providers.rs` — 569 LOC
- `src/nodes/extract/pptx_parser.rs` — 526
- `src/nodes/extract/docx_parser.rs` — 471
- `src/nodes/file/archive.rs` — 455
- `src/nodes/ai/embeddings.rs` — 431
- `src/nodes/transform/data.rs` — 419
- `src/storage/sql_store/mod.rs` — 417
- `src/nodes/image/image_basic.rs` — 407
- `src/nodes/notify/email.rs` — 406
- `src/nodes/composition/conditional.rs` — 403

Natural seams: provider adapters; OOXML model/parser/render layers; individual
archive/transform operations; SQL schema/query/codec layers; email transports;
conditional parsing/evaluation. Also remove duplicated Lua builders and global
setup. Split where it reduces responsibility/test scope, not solely due to LOC.

Progress on 2026-07-22: the duplicated Lua converters/global JSON setup were
consolidated into `src/lua/conversion/` with separate public wiring,
Lua-to-JSON traversal, JSON-to-Lua traversal, and path/index modules; every file
is below 300 LOC. IF-004 also split the former 312-line executor into dedicated
engine entry-point, coordinator, workflow, finalizer, scheduler, task-runner,
and error-handler modules; at that checkpoint every executor file was below 230
LOC.

Resolution: the ten named high-risk modules were converted into small facades
and responsibility-specific submodules for provider configuration/adapters,
OOXML theme/numbering/comments/notes/relationships/slides, archive operations,
data transforms, SQL state-store concerns, email transports, and conditional
evaluation. The same pass split semantic/fixed chunking, HTTP request assembly,
Word extraction, tool dispatch, sensitive-URL handling, image operations,
CSV/XML, subtitles, and PDF merge/split where a natural seam existed.

The resulting repository has no Rust source file above 400 lines, and every
originally named candidate is at most 290 lines per file. A small set of
cohesive implementations and files whose size is dominated by inline tests
remain between 300 and 400 lines; IF-028 does not claim that line count alone
is a defect. Focused review confirmed stable node exports/registrations and
added seam-level regressions for DOCX numbering and OpenAI-compatible/Azure
provider routing. The review also found and fixed a latent ragged embedding
response panic by requiring every returned vector to have the same dimension.

### IF-029 — JSON run pages still examine the filesystem catalog

**Status:** Resolved on 2026-07-24.

Before this resolution, the IF-021 cursor contract bounded every public page
and kept JSON selector memory at O(page size), but filenames and revision/
digest-linked summary sidecars did not form a durable ordered index.
`JsonStateStore` therefore examined O(catalog) filesystem entries for every
newest-first page, including deep cursor pages.

The required change was a crash-safe ordered summary catalog with revision/
rebuild semantics consistent with the main record and summary sidecar
lifecycle, while preserving the bounded cursor contract, canonical run-ID and
symlink protections, and an offline recovery path. This was a performance and
operability follow-up; IF-021 already prevented an API or CLI caller from
requesting or retaining the entire catalog in one response. IF-020's
revision/digest-linked sidecar repair protected correctness but did not create
this ordered index.

Resolution: IF-029 introduced a checksummed fixed-record binary base with one
global and six status-specific ordered sections. IF-033 subsequently added the
bounded mutation overlay described below. A current clean cursor page probes the
base logarithmically, range-reads at most `limit + 1 + K` base records, and
merges K delta entries, where `K <= 128`. It therefore uses
O(log N + page size + K) reads and O(page size + K) memory without enumerating
the store directory. Selected records are still verified against current
authoritative primary/summary state before the page is returned.

Catalog state now uses a version-2 dirty/clean token bound to the base generation
and delta revision, directory/base/delta fingerprints, and one local no-follow
file lock. Participating store instances mark the projection dirty before
primary mutations. Ordinary initialization, status, and delete changes replace
only the bounded delta; task/context-only updates leave both base and delta
bytes unchanged. Missing, dirty, stale, truncated, or checksum-invalid metadata
rebuilds from authoritative primary records. The stopped-writer
`JsonStateStore::rebuild_run_summary_catalog()` path publishes a new base and an
empty delta.

IF-029 regressions cover deep clean pages without directory enumeration, all
status sections, cursor timestamp ties and missing timestamps, projection
stability, corrupt/missing/dirty rebuilds, cross-instance races, interrupted
deletion, and metadata symlink rejection. IF-033 adds deterministic bounded-I/O,
base/delta merge, compaction, recovery, and concurrency coverage. JSON remains a
moderate-cardinality local backend; SQL or Redis is recommended for sustained
high-write or high-cardinality workloads.

### IF-033 — JSON projection changes replace the complete run catalog

**Status:** Resolved on 2026-07-24.

Before resolution, initialization, status changes, and deletion decoded,
reconstructed, and atomically replaced the complete fixed-record catalog even
when one projection record changed. The base duplicated every run in its global
and status section, so ordinary writes consumed O(N) memory and I/O.
Task/context-only mutations already avoided the projection rewrite.

Resolution: the IF-029 fixed-record snapshot is now an immutable base, paired
with `.ironflow-run-catalog-v1.delta`. Its checksummed header and entries store
the latest upsert or tombstone for at most 128 distinct canonical run IDs,
coalesced and sorted by ID. An ordinary projection mutation reads and atomically
replaces only this O(K) overlay; repeated changes to one ID still consume one
entry. The 129th distinct ID deliberately performs one O(N) compaction, publishes
a new base plus empty delta, and starts a new generation.

Pages binary-search the selected base section, read at most
`limit + 1 + K` base records, remove entries shadowed by the delta, and merge
matching overlay upserts in cursor order. Clean reads are therefore
O(log N + page size + K), with `K <= 128`, while final page retention remains
bounded. Task/context mutations leave both projection files byte-identical.

The existing cross-process writer lock remains the serialization boundary.
Writers publish dirty state before changing the authoritative primary, durably
replace the delta or compacted base/delta, and publish the clean token last. The
version-2 state token binds independent base-generation and delta-revision IDs
plus directory/base/delta fingerprints. Missing, dirty, stale, truncated,
checksum-invalid, or generation-mismatched metadata rebuilds from authoritative
primaries. Canonical-ID validation and no-follow/regular-file checks cover the
new delta path. The explicit stopped-writer rebuild compacts into a fresh base
and empty delta. Upgrades and downgrades across state version 2 require stopping
old writers; mixed-version writers are unsupported.

The ignored release benchmark
`storage::json_store::catalog::benchmark_tests::projection_changing_write_benchmark`
makes the write-amplification change deterministic. Before IF-033, ordinary
writes replaced 344,128 bytes at 1,000 runs and 3,440,128 bytes at 10,000 runs
(about 31.0 ms and 32.9 ms in that run). After IF-033, the mutation-persistence
step in both cases decoded a 96-byte empty delta, replaced a 304-byte one-entry
delta, and left the base byte-identical (about 15.9 ms and 15.4 ms). Token
validation also performs bounded state/header/delta reads; those constant
metadata reads are outside this counter. Timing is environment-specific; the
cardinality-independent byte counts are the acceptance signal.

Regressions prove equal bounded ordinary-write I/O at 1,000 and 10,000 base
records, one-entry coalescing across 160 repeated updates, exactly one
compaction on the 129th distinct overlay ID, task/context byte stability,
global/status/cursor merge ordering, logarithmic deep-cursor lookup plus the
`limit + 1 + K` tombstone-backfill bound at 10,000 records, delta damage and
generation recovery, explicit rebuild reset, state-version rejection, symlink
defense, and two-store writer correctness across compaction. SQL and Redis
remain the recommended backends for sustained high-write workloads.

### IF-034 — Module-size policy has no automated regression guard

**Status:** Resolved on 2026-07-24.

IF-028 reduced the source inventory from ten files above 400 lines to zero and
documented the target-at-most-300/split-before-400 convention, but enforcement was
still a manual audit. Thirteen cohesive or inline-test-heavy files currently
remain between 301 and 400 lines, so a blunt universal 300-line failure would
encourage mechanical splitting rather than better boundaries.

Resolution: `scripts/check_module_size.py` scans regular, non-symlink production
modules under `src/**/*.rs`, counting physical lines consistently even without
a final newline. Every valid-policy evaluation prints the 20 largest modules in
deterministic order, whether the source policy passes or fails.
Files through 300 lines pass directly; files from 301 through 400 require an
exact path/count ceiling and substantive rationale in
`scripts/module_size_policy.json`; any file above 400 fails unconditionally.
Missing, renamed, newly unlisted, grown, reduced, or now-small exceptions fail
until the policy is ratcheted to the new state. The current exception budget and
13 exact ceilings are review-visible, and the checker refuses a budget above
the fixed IF-034 baseline. A new exception must therefore retire an existing
one instead of expanding the set.

The checker explicitly says that LOC is a review trigger rather than a design
score and calls out responsibility boundaries, cognitive complexity, and useful
test extraction. Its 16 standard-library-only tests cover deterministic reports,
300/301/400/401 boundaries, reviewed ceilings, growth and reduction ratchets,
stale entries, rationale and budget validation, `src/` scope, and portable line
counting. GitHub Actions runs both those tests and the live-repository check;
`scripts/**` is included in push path filtering. Automation verifies that a
rationale is present and conspicuous, while human review remains responsible
for deciding whether it is sound.

## Audit evidence snapshot

- Branch: `develop`.
- Baseline before remediation: formatting, all-target check, strict Clippy, and
  497 Rust tests passed under default features.
- `cargo check --all-targets --features postgres,redis` passed.
- Registry: 98 nodes; docs: 98 node pages; names matched exactly.
- Lua catalog: 125/125 passed static validation.
- Representative offline workflows passed actual execution, but static
  validation cannot prove fixtures, external transports, or runtime semantics.
- `tests/test_examples.rs` checks one README entry, not the executable catalog.

Remediation gate completed on 2026-07-22:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo check --all-targets --features postgres,redis`
- `cargo test --all-targets`
- `cargo test --doc`
- `cargo clippy --all-targets -- -D warnings`
- `cargo clippy --all-targets --features postgres,redis -- -D warnings`
- IF-004 terminalization regressions: 6/6 passed (state/context/status write
  failures, panic, cancellation, and detached waiter).
- IF-005 disposable-Redis gate passed against Redis 8.8.0 from `redis:latest`
  (`redis@sha256:234c902a2db49461a129e2d4aeff85b28cf20187ed274a67f6e50995fa713c7b`):
  the complete serial Redis-feature suite passed, including 22/22 atomicity,
  contention, fault-injection, expiry, legacy-migration, and alias regressions.
- IF-006 deadline/cancellation regressions passed 14/14: total retry budgets,
  Lua loop and child-loader preemption, async drop, retry-wait cancellation,
  Unix shell/MCP process trees, and all structured child-workflow paths. The
  complete `postgres,redis` all-target test suite also passed, and no disposable
  cancellation-test subprocess remained afterward.
- 125/125 Lua examples passed static validation.
- README hello-world and data-pipeline workflows completed with `Status:
  success`; control-flow validation and both documented live API submission
  forms succeeded.

IF-028/IF-029 closure gate completed on 2026-07-24:

- `cargo fmt --all -- --check` and `git diff --check`
- `cargo check --all-targets` under default and `postgres,redis` features
- `cargo clippy --all-targets -- -D warnings` under default and
  `postgres,redis` features
- `cargo test --all-targets` under default and `postgres,redis` features; the
  feature-enabled inventory contains 894 tests
- `cargo test --doc`
- JSON ordered-catalog regressions: 14/14 passed, including clean/deep pages
  without directory enumeration, concurrent writer/read races, corruption
  recovery, ordering, and symlink defenses
- 125/125 Lua examples passed static validation
- Rust source inventory: zero files above 400 lines; 13 cohesive or test-heavy
  files remain between 301 and 400 lines

IF-034 closure gate completed on 2026-07-24:

- Module-size checker regressions: 16/16 passed
- Live source inventory: 312 production Rust modules, 13/13 exact reviewed
  exceptions, and zero files above 400 lines
- `actionlint .github/workflows/ci.yml`
- `cargo fmt --all -- --check` and `git diff --check`
- `cargo clippy --all-targets -- -D warnings` under default and
  `postgres,redis` features
- `cargo test --all-targets` and `cargo test --doc`
