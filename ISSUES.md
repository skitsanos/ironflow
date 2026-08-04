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
| IF-035 | P0 | Resolved | Lua security | Flow-controlled Lua bytecode is loaded in binary (`bt`) mode |
| IF-036 | P0 | Resolved | Runtime safety | Extract/encoding/S3 reads bypass `IRONFLOW_MAX_*` byte limits |
| IF-037 | P0 | Resolved | Runtime safety | Deeply nested XML/YAML overflows the stack (uncatchable) |
| IF-038 | P1 | Resolved | API DoS | Parse-time Lua pattern backtracking pins runtime workers |
| IF-039 | P1 | Resolved | Nodes/security | `db_query`/`db_exec` interpolate ctx into `AssertSqlSafe` query text |
| IF-040 | P1 | Resolved | Supply chain | Vulnerable dependencies (RUSTSEC) with no CI audit gate |
| IF-041 | P1 | Resolved | Nodes/security | `http_request` allows SSRF via ctx URL and unrestricted redirects |
| IF-042 | P2 | Resolved | Engine | `IRONFLOW_MAX_CONCURRENT_TASKS` is per-run, not process-wide |
| IF-043 | P2 | Resolved | Engine | No crash/restart reconciliation; runs stay `Running` forever |
| IF-044 | P2 | Resolved | API security | API key compared in non-constant time |
| IF-045 | P2 | Resolved | API security | Flow-load parse errors disclose arbitrary local-file contents |
| IF-046 | P2 | Resolved | Engine | `max_retries` is effectively unbounded (retry storm) |
| IF-047 | P2 | Resolved | Engine | Dropped `RunHandle` cannot cancel a hung untimed node |
| IF-048 | P2 | Resolved | Engine | Task-output cap does not bound shared/final context |
| IF-049 | P2 | Resolved | Nodes | `read_file` size guard bypassed for special files |
| IF-050 | P2 | Resolved | Nodes | `base64_decode` performs unbounded arbitrary-path writes |
| IF-051 | P2 | Resolved | Storage | `prune_before` default loads the full catalog into memory |
| IF-052 | P3 | Resolved | Maintainability | Assorted consistency/operability follow-ups |
| IF-053 | P2 | Resolved | Nodes | `subworkflow` error propagation is implicitly coupled to `output_key` |
| IF-054 | P2 | Resolved | API | `/flows/run` always accepts inline flow source, so an API key implies arbitrary execution |
| IF-055 | P1 | Resolved | Storage | Concurrent SQL schema creation crash-loops a replica on first start |
| IF-056 | P2 | Resolved | CLI config | `IRONFLOW_ALLOW_ADHOC_FLOWS` parses leniently and fails open on an unrecognized value |
| IF-057 | P2 | Resolved | Nodes | `_`-prefixed context keys are not private when a child result is namespaced |
| IF-058 | P2 | Resolved | Lua runtime | Conversion ceilings have no environment override and fail with an unactionable error |
| IF-059 | P1 | Resolved | Scheduler | No way to run a flow on a schedule |
| IF-060 | P1 | Resolved | Nodes | No way to read a spreadsheet |
| IF-061 | P1 | Resolved | API security | Flow admission and HTTP redirect trust boundaries are incomplete |
| IF-062 | P1 | Resolved | Engine/storage | Run ownership and crash recovery are not replica-safe |
| IF-063 | P1 | Resolved | Resource safety | Transcription, S3, and XLSX work bypass end-to-end ceilings |
| IF-064 | P1 | Resolved | ZIP/security | ZIP work outlives cancellation and extraction follows destination symlinks |
| IF-065 | P1 | Resolved | Extraction/runtime | Non-XLSX extractors block async workers and lack end-to-end limits |
| IF-066 | P1 | Resolved | Engine/resource safety | Extraction output and ZIP metadata amplify before memory caps apply |
| IF-067 | P2 | Resolved | Tooling/performance | Extraction resource behavior has no repeatable benchmark harness |
| IF-068 | P1 | Resolved | Artifact security | Local artifact reads trust a mutable pathname and do not verify content identity |
| IF-069 | P1 | Resolved | Binary/PDF safety | Legacy file materialization and PDF merge amplify before their limits apply |
| IF-070 | P2 | Resolved | Release governance | `main` requires a check that cannot run before merge |
| IF-071 | P2 | Resolved | Test reliability | Cancellation cleanup test signals completion before staging cleanup |
| IF-072 | P2 | Resolved | Test reliability | Lease-loss test can race its first heartbeat under load |
| IF-073 | P1 | Resolved | Scheduler availability | A hung evaluation or dead scheduler task silently stops every schedule |
| IF-074 | P1 | Resolved | Scheduler contract | Schedule configuration and cron semantics are not bounded or portable |
| IF-075 | P2 | Resolved | Scheduler performance | Schedule claim cleanup is linear on every fire |
| IF-076 | P1 | Resolved | Replica availability | Replica survival has no real-process Docker acceptance gate |
| IF-077 | P1 | Resolved | Deployment lifecycle | Serve lacks draining, readiness, and restricted-container support |
| IF-078 | P1 | Resolved | Durable admission | Retried submissions and schedule claim-to-run gaps can duplicate or lose work |
| IF-079 | P1 | Resolved | Railway deployment | Replica lifecycle has no Railway canary evidence |
| IF-080 | P1 | Resolved | OpenShift deployment | Restricted-SCC and Route behavior lack live platform evidence |
| IF-081 | P1 | Resolved | Container delivery | Hosted canaries build or deploy mutable images instead of a verified registry digest |
| IF-082 | P2 | Resolved | Tooling/resource use | Repeated integration gates retain tens of gigabytes of stale linked artifacts |
| IF-083 | P2 | Resolved | Container performance | Package-version and source changes invalidate every Rust dependency layer |
| IF-084 | P2 | Resolved | CI performance | Example validation waits for macOS and compiles the Linux release binary twice |
| IF-085 | P2 | Resolved | Hosted acceptance | CI consolidation and container caching lack hosted timing evidence |
| IF-086 | P2 | Resolved | CI performance | Independent checks and storage jobs compile the same Rust graph repeatedly |
| IF-087 | P2 | Resolved | Container performance | Warm dependency reuse transfers a 755 MB gzip layer |
| IF-088 | P2 | Resolved | Test reliability | Detached run-admission cleanup uses an undersized hosted-runner deadline |
| IF-089 | P2 | In progress | Release performance | Tag-scoped Windows caches force both release variants to rebuild dependencies |
| IF-090 | P1 | Resolved | Dependency security | Informational advisories do not fail CI and unused defaults retain unsafe or abandoned crates |
| IF-091 | P1 | Resolved | PDF/resource safety | PDF extraction reparses input and retains the final dependency advisory |
| IF-092 | P2 | Resolved | PDF/performance | PDF resource improvements lack matched before/after RSS and timing evidence |
| IF-093 | P2 | Resolved | PDF/correctness | Non-ASCII CID glyph extraction depends only on ignored local samples |

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

## Fresh audit 2026-07-24 (IF-035+)

Second deep Rust/Lua/documentation audit on `develop` at v1.12.0, independent of
IF-001..IF-034 (which were re-verified as genuinely resolved). Baseline was
healthy: `cargo fmt --check`, `cargo clippy --all-features --all-targets`, the
default `cargo test`, and `cargo test --features postgres` all passed. The
recurring theme is that existing safety machinery (the `IRONFLOW_MAX_*` limits,
the IF-001 sandbox, the IF-006 blocking-pool offload) is not uniformly applied
to every node and API path.

### IF-035 — Flow-controlled Lua bytecode is loaded in binary (`bt`) mode

**Status:** Resolved on 2026-07-24.

`code`/`foreach` loaded handler bytecode with `lua.load(&bytecode)` in the
default `"bt"` mode, which accepts precompiled binary chunks. The
`bytecode_b64`/`transform_bytecode_b64` config fields were unauthenticated
strings, so a flow author could substitute a crafted base64 chunk and load
untrusted Lua 5.4 bytecode — a memory-safety hole that defeats the IF-001
sandbox.

Resolution: handler bytecode is now authenticated with an HMAC-SHA256 tag over a
per-process ephemeral key (new `src/lua/bytecode.rs`, HMAC built on the existing
`sha2` — no new dependency). `func.dump()` output is signed at all four
flow-parse sites in `src/lua/runtime/api.rs` (`bytecode::sign`), and
`code`/`foreach` call `bytecode::verify` before `into_function`, which rejects
any payload whose tag this process did not produce (constant-time comparison).
Because the key is ephemeral and never persisted, only bytecode compiled by the
running process can load. Regressions: `bytecode` unit tests (round-trip, forged,
tampered) and an end-to-end `code`-node test in `tests/test_lua_runtime.rs`
proving a hand-crafted `bytecode_b64` is refused while legitimate function
handlers still execute.

### IF-036 — Extract/encoding/S3 reads bypass `IRONFLOW_MAX_*` byte limits

**Status:** Resolved on 2026-07-24.

`max_file_bytes`, `max_pdf_bytes`, and `max_zip_uncompressed_bytes` were enforced
in `read_file`/`write_file` and the HTTP node, but no node under `extract/`,
`utility/encoding.rs`, or `cloud/` consulted them. `extract_pdf` used
`std::fs::read`, the OOXML extractors called `entry.read_to_string` with no
per-entry size check (a 10 KB zip bomb inflates to gigabytes), and
`base64_encode(file)`/`s3_put_object`/`s3_get_object` read whole payloads into
memory. An allocation abort is not caught by the run-level `catch_unwind`, so it
kills the process.

Resolution: a new `src/util/bounded_read.rs` provides size-bounded reads
(`read_capped`/`read_to_string_capped`, sync/async `read_file_capped`) that bound
the actual bytes read via `Read::take`, so neither an over-length regular file
nor a special file such as `/dev/zero` can stream unbounded. It is wired into
`extract_pdf` (`max_pdf_bytes`), `extract_html`/`extract_srt`/`extract_vtt`/
`base64_encode(file)`/`s3_put_object` (`max_file_bytes`), every DOCX/PPTX
zip-entry read (`max_zip_uncompressed_bytes`), and a content-length pre-flight in
`s3_get_object`. Regressions: `bounded_read` unit tests for the cap boundaries,
and `tests/test_extract_limits.rs` proving `extract_word` rejects an oversized
zip entry under a low cap yet parses it under a generous one.

### IF-037 — Deeply nested XML/YAML overflows the stack

**Status:** Resolved on 2026-07-24.

`xml_parse` builds an iterative element stack but the resulting deeply nested
`serde_json::Value` overflows the worker thread's stack when it is recursively
dropped/serialized. Probed empirically: depth 20,000 parsed, depth 200,000
aborted the process with SIGABRT (uncatchable). `yaml_parse` was found NOT
vulnerable — `noyalib`'s `serde_yaml` (libyaml-style) enforces its own nesting
limit and rejects input past ~128 levels with a clean error before `yaml_to_json`
ever recurses.

Resolution: `parse_xml_to_json` now rejects element nesting beyond
`MAX_XML_NESTING_DEPTH = 128` (matching the YAML parser's limit) with a bounded
node error, so the pathological structure is never built. Regressions in
`tests/test_xml_yaml_nodes.rs` cover moderate-depth rejection (300 → error),
pathological depth not aborting (200,000 → error, previously SIGABRT), reasonable
nesting still accepted (64 → ok), and YAML deep input rejected without a crash.

### IF-038 — Parse-time Lua pattern backtracking pins runtime workers

**Status:** Resolved on 2026-07-24.

Every API flow-parse site called the synchronous `LuaRuntime::load_flow*` inline
on the async runtime, so a pathological flow definition (e.g. `string.match`
catastrophic backtracking, which the between-instruction hook cannot preempt)
pinned an async worker thread; enough concurrent requests could stall the whole
server including `/health`.

Resolution: all API parse sites (`run_flow`, `validate_flow`, `run_webhook`)
now call the `load_flow_async` / `load_flow_from_string_async` variants, which
run the parse on the blocking pool under the same `LuaExecutionLimits` and
deadline/cancellation control IF-006 established. A wedged parse now occupies a
blocking-pool thread instead of an async worker, so the runtime (and `/health`)
stays responsive, and pure-Lua runaway loops are bounded by the instruction/time
limit. Regression `tests/test_flow_file_disclosure.rs` submits a runaway-loop
flow and asserts the request returns a bounded error rather than hanging.

Residual: a C-side pattern backtrack still runs to completion on its blocking
thread (it cannot be preempted), so a determined attacker can still consume
blocking-pool threads; fully bounding that needs a hard per-parse wall-clock kill
or restricting the `string` pattern surface, tracked as future hardening.

### IF-039 — `db_query`/`db_exec` interpolate ctx into `AssertSqlSafe` query text

**Status:** Resolved on 2026-07-24.

`db_query`/`db_exec` (and `arangodb_aql`) interpolated `${ctx.*}` directly into
the query text before passing it to `AssertSqlSafe` (which bypasses sqlx's
compile-time guard), so a ctx value from a webhook/HTTP source could alter query
structure.

Resolution: all three nodes now reject a query body containing `${ctx...}` with a
clear error directing the author to the safe binding channel — `params` (`?`/`$1`
placeholders) for SQL and `bindVars` (`@var`) for AQL, both of which continue to
interpolate and bind values safely. No example or existing test used query-body
interpolation for values; the one arangodb test that exercised it was updated to
the `@bindVar` pattern. Regressions in `tests/test_db_nodes.rs` and
`tests/test_arangodb_node.rs` prove ctx interpolation in the query body is
rejected.

### IF-040 — Vulnerable dependencies (RUSTSEC) with no CI audit gate

**Status:** Resolved on 2026-07-24.

`cargo audit` reported 7 advisories with no CI gate.

Resolution: `cargo update -p quinn-proto -p crossbeam-epoch` cleared
RUSTSEC-2026-0185 and RUSTSEC-2026-0204. A new `.cargo/audit.toml` documents a
reviewed, shrinking ignore list for the 5 remaining advisories, all transitive
and not yet fixable upstream: `quick-xml` 0.39 (RUSTSEC-2026-0194/0195, via
`comrak→syntect→plist`; our direct XML nodes use the patched 0.41) and
`rustls-webpki` 0.101 (RUSTSEC-2026-0098/0099/0104, via `aws-smithy`'s
rustls 0.21). A new `audit` CI job (`.github/workflows/ci.yml`) installs
`cargo-audit` and runs `cargo audit`, which now exits 0 and will fail on any new
advisory outside the ignore list. The unmaintained/yanked entries remain
non-failing warnings. `.cargo/audit.toml` is included in the CI path filter.

### IF-041 — `http_request` allows SSRF via ctx URL and unrestricted redirects

**Status:** Resolved on 2026-07-24.

The client set no redirect policy, so reqwest followed up to 10 redirects with no
internal-address guard, and there was no way to restrict either.

Resolution: the HTTP nodes now accept `max_redirects` (default 10; `0` disables
redirect following) and an opt-in `block_private_network` (default false).
When enabled, the initial URL and every redirect hop are refused if the host is
`localhost` or a literal private/loopback/link-local/unique-local IP (including
the cloud-metadata `169.254.169.254`), via
`url_targets_internal_network` in `src/nodes/http/helpers.rs`. The default is
opt-in because IronFlow legitimately calls internal services; the point is to
provide the previously-absent control. DNS-rebinding (a public hostname
resolving to an internal IP) needs connection-level enforcement and remains out
of scope. Regressions cover the address classifier (loopback/private/ULA/metadata
vs public) and the initial-URL block; body and CRLF limits already existed.

### IF-042 — `IRONFLOW_MAX_CONCURRENT_TASKS` is per-run, not process-wide

**Status:** Resolved on 2026-07-24.

The task semaphore is per-run and a fresh `WorkflowEngine` is built per request,
so N concurrent requests yielded N×cap concurrent node executions with no
process-level back-pressure.

Resolution: an opt-in process-wide admission semaphore
(`IRONFLOW_MAX_CONCURRENT_RUNS`, unset/`0` = unlimited) is acquired in `run_flow`
and `run_webhook` and held for the run's duration (`acquire_run_permit` in
`src/api/mod.rs`); at capacity, new run requests receive `503 Service
Unavailable` via the new `AppError::ServiceUnavailable`. Nested subworkflow
children run through the engine directly, not the API handlers, so they are
unaffected and cannot deadlock against the cap. The default is opt-in to preserve
existing throughput. Documented in `docs/CLI_REFERENCE.md`; unit tests cover the
acquire/refuse/release gating.

### IF-043 — No crash/restart reconciliation; runs stay `Running` forever

**Status:** Resolved on 2026-07-24.

`RunStatus::Stalled` was only written by the in-process finalizer, so a
`kill -9`/OOM/deploy stranded every in-flight run as a permanent `Running` zombie
that `list_runs` reported and SSE consumers waited on forever.

Resolution: `cmd_serve` now runs `reconcile_nonterminal_runs`
(`src/storage/mod.rs`) before accepting traffic. Because IronFlow does not resume
runs across restarts, any `Pending`/`Running` run present at startup is a zombie
and is marked `Stalled` (terminal, with `finished` set). Ids are gathered via
bounded summary pages, so the sweep uses memory proportional to the number of
stranded runs, not the whole catalog; a reconciliation failure is logged and does
not block startup. Regression `tests/test_state_stores.rs` proves stranded
Pending/Running runs become `Stalled`, terminal runs are untouched, and the sweep
is idempotent. Graceful shutdown (`with_graceful_shutdown`) remains a separate
follow-up; this closes the operability gap of permanently-`Running` zombies.

**Superseded by IF-062:** startup no longer treats every non-terminal record as
abandoned. Runs carry renewable ownership leases; only expired leases may be
reconciled, and reconciliation continues periodically while a server is alive.

### IF-044 — API key compared in non-constant time

**Status:** Resolved on 2026-07-24.

`request_has_api_key` used `token == expected` for both `Bearer` and
`x-api-key`, and `str` `==` short-circuits on the first differing byte (a timing
side channel).

Resolution: comparison now goes through a `constant_time_eq` helper that XOR-
accumulates over all bytes without short-circuiting (`std::hint::black_box`
guards against the optimizer reintroducing an early exit); only the key length is
observable, not its contents. Unit tests in `src/api/mod.rs` cover the helper's
equal/unequal/different-length cases and `request_has_api_key`'s accept/reject
behavior for both header forms.

### IF-045 — Flow-load parse errors disclose arbitrary local-file contents

**Status:** Resolved on 2026-07-24.

File-mode flow loads echoed the raw Lua error verbatim: `POST /flows/run` and
`/flows/validate` returned `Failed to load flow: ... malformed number near
'<token>'`, leaking file-derived tokens and confirming the path was readable.
(`resolve_flow_path` already enforces `flows_dir` containment when configured;
this leak occurs in the documented permissive mode.)

Resolution: both handlers now return a generic "Failed to load flow file" for
file-mode load failures and log the redacted detail server-side
(`log_flow_file_load_failure`/`flow_file_load_error` in
`src/api/handlers/helpers.rs`). `source`/`source_base64` modes still surface
their parse errors, since that is the caller's own input. Regression
`tests/test_flow_file_disclosure.rs` posts a file whose contents surface in the
Lua lexer error and asserts the response contains neither the parse detail nor
the file token.

### IF-046 — `max_retries` is effectively unbounded (retry storm)

**Status:** Resolved on 2026-07-24.

Validation only rejected `max_retries == u32::MAX`, so `:retries(4000000000)
:backoff(0)` passed and produced ~4.3 billion no-delay attempts, each writing
task state and publishing three events — an event-store flood that stalls the
owning phase.

Resolution: `validate_step_options` now rejects `max_retries > MAX_RETRY_COUNT`
(100) before a run initializes. Regressions in `tests/test_types.rs` cover
rejection of an excessive count (1,000,000) and acceptance of a reasonable one
(10). No example or test uses a retry count above the cap.

### IF-047 — Dropped `RunHandle` cannot cancel a hung untimed node

**Status:** Resolved on 2026-07-24.

Once a `RunHandle` was dropped, there was no cancellation path and `timeout_s`
was opt-in with no run-level default, so a node stuck on a non-terminating future
kept its coordinator task alive forever.

Resolution: an opt-in run-level deadline (`IRONFLOW_MAX_RUN_SECONDS`, unset/`0` =
none) is enforced in `RunCoordinator::spawn`. When set, a timer fires the same
cooperative cancel signal `RunHandle::cancel` uses; the coordinator then cancels
and finalizes the run as `Cancelled`, and dropping the in-flight node futures
runs their IF-006 cancel-on-drop cleanup — reclaiming a hung untimed step even
after every waiter has detached. The timer is aborted when the run finishes
normally, so completed runs leave no lingering task, and the default (no env var)
path is byte-for-byte unchanged. Regression `tests/test_run_deadline.rs` proves a
30 s step under a 1 s deadline is cancelled in about a second rather than
hanging.

### IF-048 — Task-output cap does not bound shared/final context

**Status:** Resolved on 2026-07-24.

`IRONFLOW_MAX_TASK_OUTPUT_BYTES` truncated only persisted task history; the full
oversized value still entered the final durable context, so large node outputs
produced a multi-hundred-MB run document.

Resolution: the finalizer now passes the redacted final context through
`output::bound_context`, which replaces any individual value whose serialized
form exceeds the per-task-output limit with a truncation marker while preserving
small values. This bounds only the durable end-of-run snapshot used for
inspection; the in-flight context that carried full values between steps during
execution is deliberately unchanged, so data flow is not affected. Unit test in
`src/engine/executor/output.rs` verifies oversized values are truncated and small
ones preserved.

### IF-049 — `read_file` size guard bypassed for special files

**Status:** Resolved on 2026-07-24.

The guard trusted `metadata().len()`, which is 0 for `/dev/zero`, fifos, and many
`/proc` files, so the subsequent read streamed unbounded.

Resolution: `read_file` keeps the fast metadata pre-flight and now performs the
actual read through the shared regular-file, no-follow bounded reader. Special
files are rejected before consumption, while actual bytes remain capped even
if a regular file grows after its metadata check. Regression
`tests/test_read_file_special.rs` (Unix-only) reads
`/dev/zero` under a small cap and asserts a bounded error within a timeout
instead of hanging.

### IF-050 — `base64_decode` performs unbounded arbitrary-path writes

**Status:** Resolved on 2026-07-24.

`base64_decode` wrote its decoded payload to `output_file` with no size cap.
Resolution: it now rejects a decoded payload larger than `max_file_bytes()`
before writing, matching `write_file`'s existing cap. Regression
`tests/test_base64_decode_limit.rs` proves an 8 KB decode over a 1 KB cap is
rejected and the file is not created. (Path confinement remains a broader
`flows_dir`-jail decision shared with the other unrestricted file nodes and is
out of scope here.)

### IF-051 — `prune_before` default loads the full catalog into memory

**Status:** Resolved on 2026-07-24.

`StateStore::prune_before`'s default called `list_runs(None)`, materializing
every run's full record; only SQL overrode it. JSON and Redis inherited the
O(N)-memory scan.

Resolution: a shared `prune_before_via_summary_pages` helper
(`src/storage/mod.rs`) walks bounded newest-first summary pages (256 at a time),
deleting terminal runs older than the cutoff with O(page) memory. It is
delete-safe because the keyset cursor is anchored on `(started, id)`, so removing
an already-visited run does not shift later pages. `JsonStateStore` and
`RedisStateStore` now override `prune_before` to call it. Regressions in
`tests/test_state_stores.rs` verify the JSON store removes only old terminal runs
and keeps non-terminal and newer ones.

### IF-052 — Assorted consistency/operability follow-ups

**Status:** Resolved on 2026-07-31. **Done:** (a) CI toolchain bumped to
`1.97.1` across all jobs to match `rust-toolchain.toml`; (c) the finalizer's
`ContextUpdated` event is now stamped with the resolved terminal status instead
of a misleading `Running`; (e) flow validation now rejects a routed step with no
dependencies (previously silently always-skipped); (g) the JSON, Redis, and SQL
stores now preserve the first terminal transition's `finished` timestamp
(matching `NullStateStore`) rather than overwriting it on a repeated terminal
write — JSON/Redis guard on `finished.is_none()`, SQL uses `COALESCE`; regression
in `tests/test_state_stores.rs`; (b) the Lua `env()` global now honors an opt-in
`IRONFLOW_ENV_ALLOWLIST` (comma-separated names) — when set, `env()` returns
`nil` for any other key; unset preserves the documented read-any default, so no
existing flow breaks (`env_lookup` in `src/lua/sandbox.rs`, regression in
`tests/test_env_allowlist.rs`). **Deferred / by-design:** (d) deadline
completion race (the clean
fix is a grace-margin behavior change to the hardened timeout core; IF-006's
cooperative cancellation already covers the main poll-monopolizing case), (f)
orphaned temp-file startup sweep (cosmetic; no correctness impact), (h) `pcall`
budget evasion (a documented cooperative-cancellation boundary with no clean
Lua 5.4 interrupt), (i) stale `docs/superpowers/` node counts (historical
point-in-time records, intentionally not rewritten).

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
### IF-053 — `subworkflow` error propagation is implicitly coupled to `output_key`

**Status:** Resolved on 2026-07-27.

`SubworkflowNode` only propagated a failed child run when `output_key` was
unset:

```rust
if !child_succeeded && output_key.is_none() {
    return Err(...);
}
```

Namespacing a child's output and choosing an error policy are orthogonal
concerns, so this coupled two unrelated decisions. Adding `output_key` purely to
keep a child's keys out of the parent context also silently disabled error
propagation: the failed child's partial context was merged, the parent step
reported success, the run finished `Success`, and a downstream step read an
empty value and carried on. The parent logged nothing — only the child's own run
recorded the failure — so the resulting empty output was hard to trace. Unlike
`parallel_subworkflows`, which has had `on_error: fail_fast|ignore` since it
was introduced, `subworkflow` offered no way to namespace output *and* fail
fast.

Observed downstream: a flow whose LLM subworkflow failed wrote an output
artifact containing zero parsed items and reported success, so the empty result
was indistinguishable from a genuine empty input.

**Resolution.** `subworkflow` accepts `on_error` with the same vocabulary and
semantics as `parallel_subworkflows`:

- `"fail_fast"` — the parent step errors when the child run fails;
- `"ignore"` — the parent step succeeds and the caller inspects the outcome.

When `on_error` is omitted the previous coupling is preserved exactly
(`fail_fast` without `output_key`, `ignore` with it), so no existing flow
changes behaviour. A tolerated failure is now logged at `WARN` by the parent,
`subworkflow_success` is always reported (checkable without `output_key`), and
`{output_key}_error` carries the failure description when namespaced.

**Acceptance:**

- `on_error = "fail_fast"` errors the parent step even when `output_key` is set;
- `on_error = "ignore"` tolerates a failed child even when `output_key` is unset;
- omitting `on_error` reproduces the historical behaviour in both directions;
- an invalid `on_error` is rejected with an actionable message;
- `subworkflow_success` is present on success and failure; `{output_key}_error`
  only on failure;
- `docs/nodes/subworkflow.md` documents the policy, the legacy default, and the
  footgun.

Regression coverage in `tests/test_subworkflow_node.rs` (10 tests, including the
two pre-existing ones that pin the legacy defaults).

### IF-054 — `/flows/run` always accepts inline flow source

**Status:** Resolved on 2026-07-27.

`POST /flows/run` takes a flow as `file`, `source`, or `source_base64`. `file` is
canonicalised and rejected unless it resolves under `flows_dir`. The two inline
forms had no equivalent boundary — by design, since the caller supplies the
workflow.

The consequence is that possessing an API key is equivalent to arbitrary code
execution on the server: the caller chooses the nodes, so `read_file` reaches any
path the process can read (including `/proc/self/environ`, which exposes every
credential in the environment), `write_file` reaches any path it can write
(including `flows_dir` itself, making the next request a planted flow), and
`shell_command` is simply available.

For a general-purpose engine that is the contract. For a deployment that exposes
a fixed set of flows to consumer applications — where the key is shared with
those applications — it is a much larger grant than intended, and there was no
way to reduce it.

**Resolution.** `allow_adhoc_flows` (config) / `IRONFLOW_ALLOW_ADHOC_FLOWS`
(env, takes precedence), default `true`. When false, `/flows/run` rejects
`source` and `source_base64` with `403 Forbidden` and serves only `file`, which
keeps its existing `flows_dir` confinement. Webhooks are unaffected — they name a
flow from config. Defaulting to `true` leaves every existing deployment unchanged.

The original resolution left `/flows/validate` ungated because it does not
execute workflow steps. IF-061 supersedes that exception: validation evaluates
top-level Lua (including `env()`), so both inline endpoints now share the same
ad-hoc-flow policy and flow-loading admission limit.

**Acceptance:**

- inline `source` and `source_base64` are refused with 403 when disabled;
- `file` execution still succeeds when disabled;
- the `flows_dir` boundary is still enforced for `file` when disabled;
- inline source still works by default.

Regression coverage in `tests/test_adhoc_flow_policy.rs` (5 tests).

### IF-055 — Concurrent SQL schema creation crash-loops a replica on first start

**Status:** Resolved on 2026-07-27.

`SqlStateStore::new_with_prefix` and `SqlEventStore::new_with_prefix` call
`ensure_schema`, which issues `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT
EXISTS`. On Postgres those are **not atomic**: the existence check and the create
are separate steps, so two connections can both observe "absent" and one then
fails against the catalog's own unique indexes —
`duplicate key value violates unique constraint "pg_type_typname_nsp_index"` for
tables, `"pg_class_relname_nsp_index"` for indexes.

This is not a rare interleaving. Two server processes started simultaneously
against an empty database failed **5 out of 5 times**, one process exiting with
`Failed to create SQL runs table`. In Kubernetes that is a replica crash-looping
on first deploy, and with an `--atomic` Helm release it can roll the whole
release back. It only reproduces on a *fresh* schema, so it is invisible in any
environment that has already been initialised — including single-replica ones
that later scale up.

**Resolution.** `storage::sql_ddl::create_if_absent` executes a
`CREATE ... IF NOT EXISTS` statement and treats "another session created this
first" as success: Postgres `42P07` (duplicate_table), `42710`
(duplicate_object), `42P06` (duplicate_schema), and `23505` (the catalog
collision surfaces as a unique violation), plus a message fallback for drivers
that do not set a code. It is used only for CREATE, so swallowing `23505` cannot
mask a data-path conflict. All ten `CREATE` statements in the state and event
schemas now route through it, including `ensure_event_sequence_index`, which was
the second failure once the tables were fixed — its post-create verification
still runs, so a tolerated duplicate is accepted only after the existing index is
confirmed to have the right shape.

The `ALTER TABLE ADD COLUMN` migration paths already re-checked for the column
after a failure and were left as they are.

**Acceptance:**

- eight concurrent `SqlStateStore` constructions against an empty schema all
  succeed;
- the same for `SqlEventStore`, which creates more objects including a unique
  index;
- a non-database error is never mistaken for a duplicate.

Regression coverage in `tests/test_sql_schema_concurrency.rs`, gated on
`DATABASE_URL` like the other Postgres suites. Verified against Postgres 16:
0/8 failures after the change, and both tests fail when the tolerance is removed.
### IF-056 — `IRONFLOW_ALLOW_ADHOC_FLOWS` fails open on an unrecognized value

**Status:** Resolved on 2026-07-27.

IF-054 added the toggle but parsed it inline in `src/cli/mod.rs`, outside the
`resolution` module that owns the IF-018 precedence contract:

```rust
.map(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "false" | "no"))
```

Every value outside that set resolved to `true`. `IRONFLOW_ALLOW_ADHOC_FLOWS=off`,
`=disabled`, or a typo such as `=flase` therefore left inline flow execution
**enabled** while the operator believed it was disabled, and nothing was logged.
`.ok()` also swallowed a non-UTF-8 value that `environment_string` would reject.
This is the opposite of the sibling security toggle, which uses
`environment_value("IRONFLOW_ALLOW_UNAUTHENTICATED_API", "either 'true' or 'false'")`
and fails startup on anything else, per IF-018: "invalid higher-priority values
fail instead of silently falling through".

Resolution: `allow_adhoc_flows` moved into `ServerConfig::resolve` and now uses
the same strict `environment_value` path, so only `true`/`false` are accepted and
anything else aborts startup with `IRONFLOW_ALLOW_ADHOC_FLOWS must be either
'true' or 'false'`. Config-file and default precedence are unchanged
(env > `ironflow.yaml` > `true`). The documented value (`false`) is unaffected;
the undocumented `0`/`no` spellings are no longer silently accepted, which is
safe because IF-054 is unreleased.

Regression in `tests/test_cli_precedence.rs`: a binary-level run with
`IRONFLOW_ALLOW_ADHOC_FLOWS=off` over `allow_adhoc_flows: false` in YAML must
exit nonzero with the strict message, and must fail before store initialization.
### IF-057 — `_`-prefixed context keys are not private when a child result is namespaced

**Status:** Resolved on 2026-07-28.

`parallel_runner` and `subworkflow` both filtered `_`-prefixed keys out of a
child's context only on the *un-namespaced* merge branch:

```rust
if let Some(output_key) = flow_config.get("output_key").and_then(Value::as_str) {
    entry.insert(output_key.to_string(), serde_json::to_value(&run_info.ctx)?);
} else {
    for (key, value) in &run_info.ctx {
        if !key.starts_with('_') { entry.insert(key.clone(), value.clone()); }
    }
}
```

With `output_key` (or `parallel_subworkflows`' `child_output_key`) set, the
child's entire context was serialized under that key with no filtering. There
was therefore no way to keep a working value out of the parent when the result
was namespaced — the documented `_` convention silently did not apply, and the
two branches disagreed about what "private" meant.

The cost is proportional to fan-out width. Every large intermediate a child
holds — an extracted document IR, a full parsed transcript, rendered input
lines — crossed the boundary once per item, and every subsequent Lua step in the
parent then paid to convert all of it, because a step handler's context is
converted whole regardless of which keys it reads. A 50-item fan-out over
document-parsing children exhausted the 100k JSON-to-Lua node budget
(`MAX_CONVERSION_NODES`) and failed the run, with no way to opt a key out short
of restructuring the parent to drop `child_output_key` entirely.

The failure was also easy to misattribute: it surfaced on a later step that
never read the offending key, and the reported path named the consumer
(`$.processed[13].result._docx_ir.blocks[172].text`) rather than the node that
produced it.

Resolution: both call sites now apply the same `_` filter on the namespaced
branch, so the convention is uniform — `_`-prefixed keys are private to the
child however the result is exposed. Namespacing a result *under* a
`_`-prefixed key is unaffected, since that name lives in the parent's context
rather than the child's; the `prepare_llm` / `_apollo` pattern continues to
work, and its regression coverage passes unchanged.

### IF-058 — Conversion ceilings have no environment override

**Status:** Resolved on 2026-07-28. Reported as
[#103](https://github.com/skitsanos/ironflow/issues/103).

`MAX_CONVERSION_DEPTH` (64) and `MAX_CONVERSION_NODES` (100,000) in
`src/lua/conversion/mod.rs` were the only ceilings in the engine with no
`IRONFLOW_MAX_*` override and no `CLI_REFERENCE` row. Every other limit — file
bytes, directory entries, task output, Lua memory, PDF pages — is configurable,
so a flow that legitimately builds a large structure had no escape hatch short
of recompiling.

The error was also hard to act on. A step handler converts the **whole
accumulated run context**, not just the keys it reads, so a large
`parallel_subworkflows` fan-out could exhaust the node budget inside a later
step that never touched the offending data:

```
JSON-to-Lua maximum node count 100000 exceeded at $.processed[13].result._docx_ir.blocks[172].text
```

The reported JSON path names the value, not the step that produced it, and the
ceiling is only reachable at scale — so it stays invisible until a run is
already expensive, and disappears again whenever caching keeps the heavy branch
from running.

Resolution: both ceilings now read `IRONFLOW_MAX_CONVERSION_DEPTH` and
`IRONFLOW_MAX_CONVERSION_NODES` through the shared `util::limits` accessors,
defaulting to the previous values so no existing deployment changes behaviour,
and both have `CLI_REFERENCE` rows. Every limit error now names the variable
that raises it, and the node-count error states that the count covers the whole
value being converted rather than only the keys the step reads. Regressions in
`tests/test_conversion_limits_env.rs` prove each ceiling is configurable in both
directions and that the error names its override.

Not addressed: attributing the payload to the step that produced it, and a
warning threshold before the cap is reached. Both need the converter to carry
execution context it does not currently receive.

### IF-059 — No way to run a flow on a schedule

**Status:** Resolved on 2026-07-29.

IronFlow could only be triggered two ways: `ironflow run` from a shell, and
`POST /webhooks/{name}`. There was no way to say "run this flow every night at
02:00". The repository's own flagship example, `examples/00-showcase/
nightly_report.lua`, advertised a cadence the engine could not provide — it
needed an external cron calling the CLI or the API.

Resolution: a `schedules:` block in `ironflow.yaml`, evaluated by a background
task inside `ironflow serve` that ticks every 30 seconds. Configuration-file
only, exactly like `webhooks:` — timing is a deployment decision, so the same
flow can run hourly in staging and nightly in production without editing flow
source. Cron is the standard five-field form; the `cron` crate parses six
fields and reads the first as seconds, so six- and seven-field expressions are
rejected rather than silently reinterpreted as something their author did not
write. Invalid configuration — bad expression, unknown zone, unresolvable flow
path, reserved context key, `grace_seconds` below the 60-second floor — fails
the process at startup, because a schedule that only fails at 02:00 is a
schedule nobody finds out about until 02:00.

Multi-replica safety rests on one new `StateStore` method, `claim_schedule`,
which returns true exactly once per instant across every process sharing a
store. Each backend uses a primitive it already had: a unique index on SQL,
`SET NX EX` on Redis, exclusive file creation on JSON, always-true on Null. The
default implementation **fails closed** — a store that cannot coordinate makes
scheduling unavailable rather than letting every replica fire the same instant.
Claims are keyed on **local wall-clock**, not the UTC instant: on a fall-back
date the same local time maps to two distinct UTC instants, so a UTC key would
treat them as different fires and run the schedule twice.

Per due instant the order is claim → grace → overlap → admission → run.
Claiming first is what makes replicas agree — one process owns the decision, so
two cannot reach different conclusions and both act. The deliberate consequence
is that a claimed instant skipped for grace, overlap or capacity is burned
rather than retried elsewhere, so a saturated server does not build a backlog.
A claim that fails with a *store error* is the exception: nobody owned that
instant, so the watermark does not advance past it and it is retried while it
remains inside its grace window.

DST needed explicit handling in both directions, and neither is what the
library does by default. Occurrences are enumerated in local wall-clock space
and then resolved: a fall-back hour fires once, on the earlier instant; a
spring-forward gap fires at the first valid instant after it rather than
skipping the day, which is what iterating in the target zone does natively —
for `0 2 * * *` in Europe/Berlin from 2026-03-28 the library yields 03-30,
03-31, 04-01 and 03-29 never appears. Because every local time inside a gap
resolves to the same real instant, the claim key is derived from the *resolved*
time, so a `*/15 * * * *` schedule fires once across the gap instead of five
times back to back.

Runs are started and detached rather than awaited. Awaiting them made the tick
loop serial, so one long flow starved every other schedule, and a flow that
never returned — there is no default step deadline — silently ended all
scheduling for the process lifetime while HTTP kept serving. The admission
permit moves into the detached task, so `IRONFLOW_MAX_CONCURRENT_RUNS` still
bounds concurrency for the run's real duration.

Scheduled runs are ordinary runs: visible in `ironflow list`, `ironflow
inspect` and the events stream, carrying the schedule's name under `_schedule`
so a run traces back to its trigger. Every skip logs the rule that caused it at
`WARN`; a lost claim logs at `debug`, because on N replicas it is the expected
outcome N-1 times per tick and `WARN` would be pure noise.

Two supporting changes, both behaviour-preserving: `resolve_flow_path` was
split so the sandbox check can be reused without an `AppState`, and the Redis
`StateStore` impl moved into its own `state_store.rs` mirroring `sql_store`,
which dropped `redis_store/mod.rs` below the size target and returned an
exception to the budget (17 → 16).

Not addressed, and each its own future decision: sub-minute schedules,
`@reboot`-style triggers, a REST API for managing schedules, backfilling
arbitrary past instants, and running the scheduler outside `serve`. Nothing
calls `prune_before`, so run retention remains manual — schedule claims prune
themselves on the claim path because there is no retention sweep to attach to.
`grace_seconds` has a 60-second floor but no ceiling, so an absurd value stops
the SQL claim table being reaped. The overlap scan is bounded at 256 candidate
runs and logs when it stops early or cannot complete; beyond that bound a
second concurrent run of the same schedule can start. The tick loop is
unsupervised — a panic inside evaluation would end scheduling silently.

### IF-060 — No way to read a spreadsheet

**Status:** Resolved on 2026-07-30.

IronFlow could extract text and structure from Word, PowerPoint, PDF, HTML and
subtitle files, and could parse CSV already held in context. It could not read
`.xlsx` — the format business data actually arrives in. An author had to export
each sheet to CSV by hand first, losing every sheet but one and every type but
text.

Resolution: `extract_xlsx`, built on `calamine` (node count 101 → 102). One call
returns every sheet as an object keyed by sheet name, plus
`<output_key>_sheet_names` in workbook order — object key order does not survive
into Lua, so a `foreach` needs the array. An optional `sheet` narrows the
extraction and the output stays keyed by sheet name even then, so downstream
code never branches on whether narrowing happened. `sheet` is resolved by JSON
type: a string is a name, a number a 0-based index, which keeps a sheet literally
named `0` reachable via `"0"`.

Cells are typed. Whole numbers become JSON integers, but only when they
round-trip exactly through `i64` — `.xlsx` stores every number as a double, so
without that a quantity column reaches Lua as `3.0`, and Lua 5.4 distinguishes
that from `3`. The bound is deliberately strict rather than `<= i64::MAX as f64`,
because that cast rounds up to 2⁶³ and admitted a value one larger than `i64`
holds. Date-formatted cells become ISO-8601 strings: `.xlsx` has no date type, so
a date is a float plus a number-format code, and emitting the serial would push
the format lookup and the 1900-epoch quirk onto every flow. Blanks and Excel
error cells both become `null`, so a consumer treats "no usable value" uniformly.

Header rules differ from `csv_parse` in two places, because spreadsheets are not
CSVs: a blank header cell becomes `column_{n}` rather than an empty-string key,
and duplicates gain `_2`/`_3` suffixes rather than overwriting — repeated group
headers are normal, and last-wins would drop real data silently.

Three things were found by testing rather than by reasoning, and each changed the
implementation.

**A 1,403-byte file with two cells killed the process.** `calamine`'s
`worksheet_range` materialises a dense array over the bounding box of the used
cells, so a workbook holding only `A1` and `XFD1048576` requested about 550 GB
and was SIGKILLed. No ceiling could catch it — every check ran after the
allocation. Under `serve` that would take down the API and every concurrent run,
from a file small enough to arrive in a webhook body, and Excel's "phantom last
cell" makes the milder shape an ordinary real-world condition rather than an
attack. The node now streams cells and never materialises the bounding box, so
memory is bounded by cells actually read: the same file now returns a clean
ceiling error at roughly baseline process memory.

**The ceilings could never fire.** `IRONFLOW_MAX_XLSX_CELLS` began at 1,000,000,
but the Lua conversion budget (`IRONFLOW_MAX_CONVERSION_NODES`, default 100,000)
always bit first, producing the JSON-path error IF-058 was filed about instead of
a message naming the sheet. Conversion cost is roughly `rows × (cols + 1)`, worst
at one column, so the ceiling is now 33,000 — a third of the conversion budget,
which holds at every width. Raising one variable without the other mostly just
moves where an oversized workbook fails.

**Streaming initially widened ordinary workbooks.** `worksheet_range` discarded
formatting-only cells before computing the bounding box; the first streaming
version did not. A sheet with two real columns and one leftover styled-blank cell
gained 24 spurious null columns. The stream now filters those records, matching
the original behaviour.

Not addressed, and each its own future decision: `.xls`, `.xlsb` and `.ods` are
readable by the underlying library but unsupported and untested; writing
workbooks; formulas as expressions rather than cached values; cell formatting,
colours and comments; charts, images, named ranges and pivot tables. A
`skip_rows` parameter is the obvious next addition — real workbooks commonly
carry a title row above the header, and with the default `has_header` that title
becomes the keys.

Known limitations, stated plainly rather than implied:

- Excel error cells collapse to `null` alongside blanks, so a flow auditing a
  workbook cannot distinguish `#DIV/0!` from an empty cell.
- Formulas yield the cached value only. A workbook written by a tool that did not
  populate cached values reads as blank.
- Merged cells report their value in the top-left cell and `null` across the rest
  of the span — the file's own representation.
- Hidden and very-hidden sheets are extracted like any other.
- The zip pre-flight, which enforces the uncompressed-bytes and entry-count
  limits `calamine` would otherwise bypass, trusts the archive's declared sizes.
  It restores parity with the other OOXML nodes; it is not a hard bound. The
  memory bound is the streaming path, not this.
- **No committed test reads a real Excel-authored workbook.** `data/samples/` is
  gitignored by policy — it holds internal data — so such a test would fail in CI
  and for every other developer. Real-file verification was done manually against
  three workbooks and is recorded in the branch's development notes.
- **Excel date serials have no real-file coverage.** None of those three
  workbooks contained a single Excel-typed date cell; one has a column named
  "FMV Approval Date" whose values are strings like `"8/30/2024, 9:01 PM"`,
  because report exports commonly pre-format dates as text. Date-serial handling
  is covered only by synthetic fixtures — one custom number-format code and one
  built-in format id end to end through a file, plus 1900-epoch boundary tests
  that construct the value directly and so bypass format detection.
- **The committed tests do not guard the original out-of-memory bug.** At any
  bounding box small enough to test automatically, the pre-fix and post-fix code
  reach the same decision and differ only in memory; the defect appears only at a
  scale that cannot be safely automated. The repro is preserved as an ignored
  test with its history in the body.

### IF-061 — API trust-boundary and admission gaps

**Status:** Resolved on 2026-07-31.

Six boundaries were independently weaker than their public contract:

- disabling ad-hoc execution still allowed caller-supplied Lua through
  `POST /flows/validate`; top-level flow Lua can read allowlisted environment
  values even though validation does not execute workflow steps;
- disabling ad-hoc flows without `flows_dir` left file mode able to resolve
  arbitrary process-visible paths;
- run admission was acquired after Lua parsing, so expensive parse work bypassed
  the process run cap, and cancelling an HTTP waiter could release either a
  parse or run permit while the underlying work continued;
- file flow loading created its Lua VM before performing an unbounded
  `read_to_string`; special files could block a worker indefinitely and regular
  files had no independent source-size ceiling;
- configured `flows_dir` failures distinguished existing outside paths from
  missing ones, exposing a filesystem-existence oracle;
- HTTP nodes followed cross-origin redirects by default. Reqwest can forward
  caller-configured authentication, headers, or request bodies, and generated
  `Referer` values can expose query-string credentials. IPv4-mapped IPv6 also
  bypassed the literal private-address classifier.

Implemented locally:

- `/flows/run` and `/flows/validate` now enforce one ad-hoc policy before
  decoding or evaluating inline source. `allow_adhoc_flows=false` requires a
  configured `flows_dir`; startup and direct handler construction both fail
  closed rather than reverting to arbitrary paths.
- `IRONFLOW_MAX_CONCURRENT_FLOW_LOADS` is a strict, positive process-wide
  semaphore with a default of two. API, webhook, and scheduler entry points
  acquire it before blocking Lua flow loading, while the existing run semaphore
  is acquired before parsing for every path that can create a run.
- Detached supervisors retain parse and run permits until the blocking parse or
  durable `RunHandle` actually settles. Aborting the request future therefore
  cannot create hidden work above either advertised ceiling.
- `IRONFLOW_MAX_FLOW_SOURCE_BYTES` (1 MiB default) bounds inline and file Lua
  before VM creation. File reads use opened-handle metadata, accept only regular
  files, read in capped chunks with cancellation checkpoints, and use
  non-blocking open on Unix so FIFOs cannot strand a loader.
- configured-root path escapes are rejected before probing caller-selected
  outside paths; existing, missing, traversal, and symlink escapes share one
  generic `404` response while detailed reasons remain server-side.
- HTTP redirects are same-origin by default and capped at 100. A cross-origin
  opt-in applies only to plain requests: configured auth, headers, or a body are
  an unconditional cross-origin fence. Generated `Referer` headers are disabled,
  retry/redirect numeric fields are strictly bounded, and private-address
  classification normalizes IPv4-mapped IPv6.

Focused regressions cover validation policy, the required `flows_dir` startup
invariant, generic webhook file-load errors, bounded regular-file reads, prompt
FIFO rejection, indistinguishable outside-path failures, aborted waiters
retaining both permit types, redirect credentials/body/header cases, same-origin
behavior, explicit safe opt-in, IPv4-mapped addresses, and strict runtime-limit
parsing.

Contract boundary: the flow-load limit bounds concurrent Lua parses, not queued
requests; the run limit owns a run from before its parse until durable completion.
These controls do not make an allowed workflow unprivileged—the node set still
has the process permissions of the IronFlow deployment.

Validation: default and combined-feature checks pass; both exact all-target
Clippy commands pass with warnings denied; the full default all-target suite,
doctests, and all 128 Lua example validations pass.

### IF-062 — Replica-safe run ownership and reconciliation

**Status:** Resolved on 2026-07-31.

IF-043 marked every non-terminal run `Stalled` on startup. That was safe for a
single stopped process but incorrect for rolling or multi-replica deployments:
one replica could terminalize work actively owned by another. Conversely, a
startup-only sweep never recovered a run abandoned after the server was already
up. Even with ownership metadata, unfenced task/context writes could let a stale
worker mutate a run after another replica reconciled it.

Implemented locally:

- each initialized run receives an opaque owner and renewable 90-second lease;
  a 30-second heartbeat emits a typed infrastructure stop when renewal times
  out, fails, or loses ownership. Infrastructure stops outrank simultaneous
  explicit/deadline cancellation and converge to durable `Stalled` state;
- owner-aware status, task, and context mutations fence stale workers. Built-in
  JSON, SQLite/PostgreSQL, and Redis stores implement the contract; third-party
  `StateStore` implementations retain compatible permissive defaults and opt out
  of automatic lease reconciliation until they override the methods;
- Redis uses atomic Lua fencing and Redis `TIME`; SQL uses transactional writes
  and the database clock. Reconciliation is bounded in 256-record batches and
  terminalizes abandoned pending/running tasks before marking the run `Stalled`;
- JSON stores leases in a separate protected namespace and serializes each
  lease read/modify/write transaction under an OS lock. Reconciliation streams
  candidates rather than holding the complete catalog or one lock across the
  backlog;
- server startup performs fail-closed reconciliation after configuration and
  binding succeed but before the scheduler starts. A supervised periodic reaper
  retries bounded reconciliation calls, so leases expiring after startup also
  converge;
- run initialization, task/context/status persistence, event publication, and
  finalization have explicit liveness budgets. Cancellation, the run deadline,
  or lease loss can preempt the whole execution future—even if a node or custom
  store future ignores cooperative cancellation—and release process admission.
- Redis active-run expiration never shortens configured retention below the
  lease safety window. With no configured retention, run state remains
  persistent throughout init, renewal, and owned mutations.
- JSON, SQL, and Redis deletion atomically refuses a live non-terminal owner
  with typed `Conflict` (`409` at the API), without removing its lease or event
  stream. Terminal and expired-lease runs remain deletable; SQL follows the
  existing lease-before-run lock order and Redis performs the decision in Lua.

Focused coverage exercises ownership loss with durable `Stalled` convergence,
typed stop precedence, stale-writer fencing, live renewal, live/expired/terminal
deletion across JSON, SQL, and Redis, API `409` event preservation, more than
one SQL reconciliation batch, reaper timeout/retry, JSON cancellation during a
commit, Redis lease TTL/persistence, API and CLI lifecycle startup, and hanging
state/event futures under run deadlines.

Contract boundary: reconciliation makes durable state converge; it cannot roll
back an external side effect that completed before a worker crashed or lost its
lease. A custom backend is replica-safe only after it implements the owned
methods and reconciliation contract. JSON coordinates only processes sharing
the same filesystem.

Validation: the required combined-feature all-target suite passes serially
against disposable `redis:latest` (Redis 8.10.0) and `postgres:latest`
(PostgreSQL 18.4), with service-required flags preventing skips. This includes
53 Redis atomicity tests, 18 Redis store tests, and live PostgreSQL event,
schema, concurrency, schedule-claim, state, lease, and deletion coverage. The
two explicitly named containers were removed after the run.

### IF-063 — End-to-end resource ceilings for transcription, S3, and XLSX

**Status:** Resolved on 2026-07-31.

Several paths performed bounded input checks but still admitted unbounded work
later in the operation. Transcription buffered arbitrary provider responses and
allowed cross-origin redirects; S3 `get_object` collected a response before
checking its final size; and `extract_xlsx` performed blocking decode on an
async worker, trusted declared ZIP sizes, materialized shared strings before its
cell ceiling, and did not bound cumulative output bytes. Its row ceiling also
counted stored records rather than the highest spreadsheet row position, so a
sparse sheet could evade the documented limit.

Implemented locally:

- transcription streams provider bodies under
  `IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES` (25 MiB default), rejecting both an
  oversized `Content-Length` and chunked overflow. Redirects are capped at ten
  and must retain scheme, host, and effective port. `temperature` is parsed
  strictly as a finite value in `0..=1`. Audio input uses the shared no-follow
  regular-file reader, successful JSON is preflighted against conversion depth
  and node ceilings before materialization, and provider error extraction is
  bounded;
- S3 downloads stream chunks under the shared `IRONFLOW_MAX_FILE_BYTES` ceiling
  instead of collecting first;
- XLSX work runs through the cancellation-aware blocking-step bridge, checks
  cancellation throughout workbook/sheet decoding, preflights shared strings
  with a streaming parser, and applies
  `IRONFLOW_MAX_XLSX_OUTPUT_BYTES` (50 MiB default) to cumulative decoded/result
  bytes. Repeated shared-string references are charged per use;
- XLSX validates classic and ZIP64 end-of-central-directory metadata before
  constructing `ZipArchive`, retains one no-follow regular-file handle across
  every preflight and decode stage, and enforces declared and actual per-part
  compressed/uncompressed plus cumulative archive ceilings;
- XLSX headers use budget-aware construction and amortized collision handling,
  selector conversions are checked, sparse positions enforce the documented
  one-based row ceiling, and the implementation is split into focused modules
  below the hard size limit.

Focused XLSX unit/integration suites cover wide rows, sparse bounds, shared
strings, output accounting, cancellation, and header collisions. Transcription
tests cover streaming overflow, redirect origin changes, Azure's two-origin
request shape, and strict temperature; S3 body tests cover declared and streamed
overflow.

Contract boundary: an archive's declared uncompressed sizes are a useful
preflight but not a trusted memory bound. The streaming parser, cell/row limits,
decoded-output budget, and cancellation checkpoints form the enforcement path.

Validation: 47 focused XLSX unit tests and 17 integration tests pass (the
documented pre-fix OOM reproducer remains intentionally ignored), followed by
the full default and live combined-feature all-target suites. `cargo audit`
passes with the four explicitly reviewed unmaintained transitive warnings, and
the 371-module policy passes with no new production module above 300 LOC.

Integration evidence for `1.16.0-dev.1` on 2026-07-31: the repository integration
gate passed formatting, the 371-module size policy, repository skill and hook
tests, `actionlint`, default and `postgres,redis` all-target checks, both exact
Clippy gates with warnings denied, the default all-target suite, doctests,
`cargo audit`, a release build, and static validation of all 128 Lua examples.
The required combined-feature all-target suite then passed serially against
freshly pulled `redis:latest` and `postgres:latest` containers with required-test
flags enabled; this included the 53-test Redis atomicity suite, 18 Redis store
tests, and live PostgreSQL event, schema, concurrency, claim, state, lease, and
deletion coverage. The gate removed its two explicitly named containers.

### IF-064 — ZIP filesystem lifecycle is neither root-confined nor cancellation-safe

**Status:** Resolved on 2026-07-31.

`zip_create`, `zip_list`, and `zip_extract` launch raw `spawn_blocking` workers
instead of the cancellation-aware blocking-step bridge. Traversal, compression,
entry reads, and extraction copies have no execution-control checkpoints. A
timed-out or cancelled run can therefore become durably terminal and release its
run-admission permit while detached ZIP work keeps consuming a blocking worker
and mutating files.

There are two path-safety defects in the same loops:

- extraction validates archive entry names lexically, then uses
  `create_dir_all` and `File::create`; a pre-existing symlink in the destination
  path—or at the leaf—can redirect/truncate a file outside the extraction root;
- creation uses metadata helpers that follow directory symlinks recursively,
  so it can archive data outside the requested source and a symlink cycle can
  recurse until failure or resource exhaustion.

Required outcome:

- route all ZIP blocking work through the shared cancellation bridge and add
  checkpoints during traversal and chunked copies, with an explicit partial
  output/cleanup policy;
- use race-safe no-follow traversal for extraction (directory-relative/openat
  semantics on Unix), reject unsafe existing parent and leaf entries, and never
  truncate an external symlink target;
- define and document a no-follow policy plus depth/work ceiling for creation;
- prove cancellation stops physical filesystem work before the relevant worker
  capacity is reusable, and add Unix parent-symlink, leaf-symlink, external-tree,
  and symlink-cycle regressions.

Implemented locally:

- all three ZIP nodes now use the tracked cooperative blocking bridge. Step
  deadlines retain task capacity until a worker physically exits, and the run
  coordinator retains external run admission until tracked cleanup is idle;
- traversal, metadata loops, compression, and extraction copies checkpoint at
  bounded entry/chunk boundaries. A one-slot blocking-pool regression proves a
  cancelled copy removes its temporary file and preserves the prior target
  before that capacity can be reused;
- ZIP creation uses iterative traversal, rejects source-root and descendant
  symlinks plus special files, enforces the shared directory-depth limit, and
  counts both files and directories against its traversal-work ceiling. Actual
  copied bytes are rechecked instead of trusting metadata alone;
- creation and extraction stage sibling temporary files. Creation publishes
  only a finished archive; extraction publishes each complete file atomically.
  Failure/cancellation removes the active temporary and preserves an existing
  leaf, while earlier committed extraction entries and directories may remain;
- extraction preflights every name, type, normalized collision, declared byte
  total, entry count, and path depth before destination mutation. ZIP symlink
  and special-file entries are rejected, and actual decoded bytes remain
  bounded during copying;
- Unix extraction pins the destination with directory descriptors, walks every
  archive-controlled component with `openat`/`mkdirat` and `O_NOFOLLOW`, checks
  leaves with `fstatat`, and commits with `renameat` or no-replace
  `linkat`/`unlinkat`. The portable fallback rejects observed symlinks but
  documents its weaker concurrent-race boundary;
- archive inputs use the shared no-follow regular-file opener, output symlinks
  are rejected, duplicate and non-portable archive destinations fail, and the
  ZIP node docs plus bounded Lua workflow now match the implemented contract.

Focused validation: 23 public ZIP tests pass across the existing file-node and
new cancellation, traversal-limit, and Unix security suites. Four private
tests cover chunk cancellation, physical worker lifetime, temporary cleanup,
and descriptor anchoring; the 12 step-timeout/engine-terminalization tests also
pass. The rebuilt binary validates all 128 Lua examples, the three example
catalog tests pass, formatting and diff checks are clean, the 379-module size
policy passes with every new production module below 300 lines, and
`cargo clippy --all-targets -- -D warnings` passes. Per the ordinary-change
policy, the unrelated full integration suite was not run.

## Extraction resource baseline before IF-065 (2026-07-31)

The release binary at `fac37b6` (`1.16.0-dev.2`) was profiled on macOS 26.5.2,
an Apple M4 Pro with 12 logical CPUs and 24 GiB RAM. Each case ran in a fresh
JSON state directory under `/usr/bin/time -lp`; the table reports the median of
three runs. CPU is user plus system time. These short wall times are coarse and
include a roughly 0.3-second CLI/startup/storage floor, so the useful signals
are peak RSS, output amplification, limit behavior, and concurrency rather than
small timing differences.

`Output` is the compact serialized task output before the executor's 2 MiB
history truncation. `data/samples/` is gitignored and machine-local, so these
figures are calibration evidence, not a reproducible CI gate.

| Case | Input / shape | Wall | CPU | Peak RSS | Output |
|---|---:|---:|---:|---:|---:|
| Empty workflow baseline | — | 0.32 s | 0.03 s | 17.1 MiB | 11 B |
| XLSX small | 177,369 B / 432 cells | 0.26 s | 0.04 s | 18.5 MiB | 3,311 B |
| XLSX near default ceiling | 354,086 B / 32,014 cells | 0.33 s | 0.10 s | 59.7 MiB | 694,273 B |
| XLSX raised ceiling | 720,564 B / 85,908 cells | 0.41 s | 0.16 s | 98.3 MiB | 1,375,676 B |
| Four concurrent XLSX steps | 4 x 32,014 cells | 0.72 s | 0.38 s | 181.1 MiB | about 2.65 MiB |
| PDF text + metadata | 108,505 B / one page | 0.26 s | 0.03 s | 21.6 MiB | 3,277 B |
| Word JSON + metadata/comments | 19,842 B | 0.26 s | 0.04 s | 23.9 MiB | 63,241 B |
| VTT text + cues/metadata | 11,097 B / 80 cues | 0.25 s | 0.03 s | 18.6 MiB | 32,425 B |
| PPTX text | 38,741,985 B / 86 slides | 0.29 s | 0.06 s | 21.0 MiB | 109,757 B |
| PPTX JSON, no image bytes | same deck | 0.28 s | 0.05 s | 40.4 MiB | 193,002 B |
| PPTX JSON with image bytes | same deck | 0.34 s | 0.10 s | 231.3 MiB | 36,404,071 B |
| PPTX text with image bytes requested (pre-IF-065) | same deck | 0.32 s | 0.09 s | 98.2 MiB | 109,757 B |

The 85,908-cell workbook is correctly rejected by the default 33,000-cell
budget; its successful row above used `IRONFLOW_MAX_XLSX_CELLS=100000`. A Lua
consumer would also need a coordinated conversion-node budget. The four-way
case used distinct output keys and `IRONFLOW_MAX_CONCURRENT_TASKS=4`. The
1.84 MiB `generated_book.pdf` sample is image-only for this extractor and
returned no text, so it is not a useful PDF stress case.

The PPTX contains 536 ZIP entries and 119 media files (about 26 MiB
uncompressed). Its 102 resolved image occurrences refer to 85 unique paths; one
image is read and encoded eleven times. When image bytes are enabled, the full
36.4 MiB result is materialized and copied before durable output is replaced by
the 2 MiB truncation marker. At the profiled pre-remediation commit, requesting
image bytes with text output produced the same 109,757-byte result as ordinary
text while raising median RSS from 21.0 to 98.2 MiB. The current contract
rejects `include_image_bytes = true` for every format and directs JSON callers
to `media_mode = "artifact"`.

A post-remediation release build from the final IF-065 working tree was checked
against the same 37 MiB deck with fresh state directories (median of three
runs). This is a compatibility/resource spot check, not a benchmark gate:

| Current IF-065 case | Wall | CPU | Peak RSS |
|---|---:|---:|---:|
| PPTX text | 0.27 s | 0.04 s | 20.4 MiB |
| PPTX JSON, no image bytes | 0.29 s | 0.05 s | 38.7 MiB |
| PPTX JSON with image bytes | 0.33 s | 0.10 s | 247.0 MiB |

Text and metadata-only JSON remained near or below the baseline footprint.
Embedded-image JSON remains the dominant allocation path and measured higher
in this spot check despite similar CPU and wall time. IF-065 provides bounded
work, explicit failure, and cancellation lifecycle guarantees; it does not
claim an RSS reduction. Repeated-media and output-copy amplification therefore
remain P1 work under IF-066, while IF-067 owns reproducible trend measurement.

### IF-065 — Non-XLSX extraction blocks async workers and lacks end-to-end limits

**Status:** Resolved on 2026-07-31.

`extract_html`, `extract_pdf`, `extract_word`, `extract_pptx`, `extract_srt`,
and `extract_vtt` perform synchronous file reads, ZIP work, parsing, and output
construction directly inside `async fn execute`. None uses the shared
`run_blocking_step`/`ExecutionControl` path that `extract_xlsx` already uses. A
PPTX step with a 1 ms timeout still occupied its runtime worker for about 300 ms
and reported the timeout only after parsing returned: the deadline bounded
publication, not physical CPU or memory work.

The format ceilings are also incomplete:

- HTML and subtitles cap only the raw UTF-8 file at 50 MiB. HTML can parse and
  copy it several times; subtitles always materialize cue-owned text, a plain
  transcript, cue JSON, and possibly a second formatted transcript. There is no
  output or structural-count budget.
- PDF caps the raw file at 100 MiB but not pages, decoded streams, extracted
  text, or output. Text mode clones the extracted text, and requesting metadata
  parses the PDF a second time.
- DOCX/PPTX construct `ZipArchive` from an ordinary followed path before a raw
  file, entry-count, or central-directory guard. The shared ZIP byte limit is a
  per-read ceiling, not a cumulative archive budget, and PPTX commonly converts
  a limit/read failure into an empty part and silently returns partial output.
- DOCX comment anchoring appends each text event to every open comment range,
  allowing adversarial work proportional to open ranges times text events.

This also contradicts the current documentation: `NODE_REFERENCE.md` broadly
claims node execution runs on the blocking pool; `CLI_REFERENCE.md` omits HTML
and subtitles from `IRONFLOW_MAX_FILE_BYTES`, describes the PDF limit as
render-only, and implies the ZIP totals apply to every archive node. The
individual non-XLSX node documents state no resource or cancellation contract.

Required outcome:

- move every synchronous extractor behind the cancellation-aware blocking
  bridge, thread checkpoints through controllable loops, and retain physical
  worker admission until work really stops;
- generalize the no-follow regular-file and archive preflight used by XLSX for
  DOCX/PPTX, enforcing raw size, central-directory/entry count, cumulative
  declared and actual decoded bytes, and explicit failure instead of partial
  success on a breached limit;
- add format-appropriate structural and output budgets (pages/text, blocks and
  comments, slides/elements/media, cues, and HTML result bytes), charging before
  expensive allocation where possible;
- reject present-but-invalid configuration types consistently, including
  `extract_xlsx.has_header`, rather than silently selecting a default;
- add focused FIFO/symlink, cumulative archive, timeout/cancellation, output,
  and adversarial-count regressions, then align all affected node and CLI docs.

Keep the implementation split by format and responsibility; the resource
policy must not turn any extractor into a 300-400-line coordinator.

Implemented locally:

- `extract_html`, `extract_pdf`, `extract_word`, `extract_pptx`, `extract_srt`,
  and `extract_vtt` now run synchronous filesystem, archive, parser, and output
  work through the tracked cooperative blocking bridge. Controllable loops
  checkpoint deadlines and cancellation, and task/run admission remains held
  until physical worker cleanup completes;
- raw inputs use the shared regular-file/no-follow boundary. DOCX and PPTX
  preflight EOCD/ZIP64 metadata before constructing `ZipArchive`, reject
  duplicate, symlink, and special-file parts, and enforce entry-count,
  cumulative declared-byte, cumulative actual decoded-byte, and per-part
  ceilings. Present malformed or unreadable parts fail explicitly instead of
  producing partial output;
- `IRONFLOW_MAX_EXTRACT_ITEMS` and
  `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` provide cumulative structural/work and
  complete serialized-result limits for all six non-XLSX extractors.
  `IRONFLOW_MAX_PDF_EXTRACT_PAGES` bounds PDF page count before text extraction;
- controllable subtitle, PDF Markdown, and DOCX formatting charges generated
  output before allocation and checkpoints throughout its loops. DOCX comment
  fan-out is charged before expansion. PPTX no longer clones the complete slide
  graph to attach comments, reads image bytes only for explicitly requested
  JSON output, and charges Base64 size before encoding;
- present invalid configuration types and colliding output keys fail before
  filesystem work, including `extract_xlsx.has_header`. Subtitle parsing keeps
  the canonical plain `transcript` while requiring a distinct key for optional
  formatted output;
- the CLI and node references, all seven extractor documents, and the affected
  Lua examples describe the implemented raw, decoded, structural, output,
  persistence, malformed-input, and cancellation boundaries.

Contract boundary: these are logical work/result ceilings rather than a
process-RSS guarantee. Opaque third-party HTML and PDF calls can still allocate
a result before IronFlow inspects it, although they are bracketed by
checkpoints and worker admission remains held until they stop. Executor output
copies, XLSX metadata/cell amplification, and repeated PPTX-media processing
remain tracked by IF-066; repeatable CPU/RSS benchmarking remains IF-067.

Focused validation: `cargo test --lib nodes::extract` passed 52 tests. The ten
extractor integration targets passed 47 tests with only the documented pre-fix
XLSX OOM reproducer intentionally ignored. This includes deterministic
mid-flight cancellation that gates a real extractor worker and proves run
admission remains held until physical drain, plus formatting-amplification,
configuration, PDF, HTML/subtitle, DOCX, PPTX-limit, and PPTX-media regressions.
Static validation passed for all 128 Lua examples, the six changed extraction
examples executed successfully against isolated fixtures, and the three
example-catalog tests pass. Formatting and diff checks are clean, the
393-module size policy passes with every new production module below 300 lines,
and `cargo clippy --all-targets -- -D warnings` passes. Per the ordinary-change
policy, the unrelated full integration suite was not run.

### IF-066 — Extraction amplification occurs before memory ceilings apply

**Status:** Resolved on 2026-07-31.

IronFlow now uses a content-addressed local artifact store for binary workflow
handoff. `ArtifactRef` carries a canonical `artifact://sha256/<digest>` URI,
digest, size, and an optional validated MIME type. Publication streams into a
private staging file, enforces byte and cancellation limits, hashes before atomic
publication, deduplicates verified content, and never places Base64 bytes in
the workflow context for that artifact payload. `read_file` can publish an
artifact directly; PPTX, `pdf_to_image`, and `pdf_thumbnail` return
descriptors; image consumers and artifact-enabled PDF extraction, rendering,
metadata, and split consumers accept those descriptors through bounded source
triage. PPTX rejects
the deprecated `include_image_bytes = true` contract in every format and uses
`media_mode = "artifact"` for media extraction.

The earlier amplification paths were also removed or admitted before ownership:

- raw XLSX central-directory traversal validates the exact declared boundary,
  caps cumulative names/extra fields/comments at 8 MiB by default, detects
  duplicate part names using fixed-size digests plus exact collision checks,
  and runs before `ZipArchive` or Calamine construction;
- XLSX cell cost and retained output are charged before borrowed values become
  owned, selectors and sheet diagnostics are bounded, and the archive/input
  admission precedes `DataRef` ownership;
- DOCX and PPTX XML parts are consumed through bounded streaming readers rather
  than complete part `Vec`/`String` allocations; declared and actual ZIP bytes,
  entry counts, duplicate names, CRC completion, UTF-8, and cancellation remain
  enforced;
- PPTX resolves only valid internal image relationships, validates package MIME
  data before publication, caches media by normalized archive path, and streams
  each unique media part once into the artifact store;
- executor output preparation has an empty-redactor fast path, a bounded
  counting serializer that stops beyond the persistence threshold, and moves
  uniquely owned phase values instead of cloning them;
- image inputs are admitted by encoded bytes, dimensions, working allocation,
  source count, and cumulative `image_to_pdf` limits before Base64 ownership or
  pixel decoding. Image work, PDF metadata/split, and Pdfium rendering execute
  on tracked blocking workers with cancellation-aware, growth-aware capped
  readers.

A final release build was measured against the same ignored 38,741,985-byte
`data/samples/sample.pptx` used for IF-065, with fresh state and artifact
directories for each run. This is a local compatibility/resource spot check,
not a portable benchmark gate:

| Run | Wall | User + system CPU | Peak RSS | Unique artifacts | Artifact bytes on disk |
|---:|---:|---:|---:|---:|---:|
| 1 | 1.32 s | 0.28 s | 39.8 MiB | 85 | 24.4 MiB |
| 2 | 1.18 s | 0.27 s | 35.5 MiB | 85 | 24.4 MiB |
| 3 | 1.33 s | 0.34 s | 35.1 MiB | 85 | 24.4 MiB |

Median peak RSS was 35.5 MiB, an 85.6% reduction from the 247.0 MiB IF-065
inline-Base64 spot check. Every run published the same 85 unique artifacts and
left no staging `.tmp` files. The additional wall time is the deliberate cost
of streaming, hashing, syncing, and atomically publishing 85 files.

Focused validation passed 67 extractor library tests; 55 extractor integration
tests with the documented pre-fix XLSX reproducer still ignored; 50 artifact,
image, executor, and redaction library tests; 48 artifact/image/PDF and
executor/recovery/security integration tests; three catalog tests; and the
isolated example runtime matrix. All 129 Lua examples validate, Rust doctests
complete successfully, formatting and `git diff --check` are clean, the
425-module size policy passes, and
`cargo clippy --all-targets -- -D warnings` passes. Per the ordinary-change
policy, the unrelated full integration suite was not run.

Remaining boundaries are explicit rather than hidden by this closure. The
local artifact directory is a trusted same-process-identity store and read-side
path identity still requires IF-068. `write_file` Base64 and `pdf_merge`
materialization remain IF-069. Repeatable cross-machine RSS/CPU trend tooling
remains IF-067. Artifacts have no automatic garbage collection, transaction
rollback, or multi-host replication; Calamine, codecs, `lopdf`, and `quick-xml`
may retain bounded internal models or one token-sized allocation; logical byte
limits are safety contracts, not exact process-RSS promises. The two
Pdfium-dependent integration cases retain a capability-skip path when the
native library is unavailable, so this focused gate does not claim live native
Pdfium coverage.

### IF-067 — Extraction resource behavior has no repeatable benchmark harness

**Status:** Resolved 2026-08-02 (found 2026-07-31).

The committed extraction fixtures remain intentionally small and functional;
`data/samples/` is gitignored, and examples must not depend on it. IF-065 added
deterministic regressions for cumulative declared and actual OOXML bytes,
structural and serialized-output limits, malformed parts, overlapping DOCX
comments, subtitle work, regular-file boundaries, and mid-flight physical
cancellation/drain behavior. It also documented each extractor's current
resource and persistence contract.

Those regressions prove rejection and lifecycle semantics, not peak CPU/RSS,
concurrent amplification, or post-cancellation drain time. The XLSX pre-fix OOM
reproducer remains ignored and measures elapsed time rather than resources.
There is still no repeatable release-mode subprocess benchmark for comparing
machines or detecting broad memory-shape regressions.

Required outcome:

- add an opt-in release-mode subprocess benchmark that accepts an explicit
  samples directory and emits JSON/JSONL containing input checksum and raw/
  declared bytes, status/limit, wall and user/system CPU time, peak RSS,
  serialized output and persisted bytes, plus post-cancellation drain time;
- include an empty baseline, configurable repetitions, and concurrency 1/2/4;
  keep machine metadata with each result and never include document content;
- use local ignored samples for calibration while adding compact deterministic
  fixtures for long ZIP metadata, repeated PPTX media, compressed PDF text,
  pathological HTML, and concurrent extraction. Existing cumulative OOXML,
  DOCX comment-fan-out, subtitle, cancellation, and logical output-limit
  fixtures need not be duplicated;
- document the benchmark command, result schema, machine metadata, and
  interpretation rules. Keep trends opt-in and use only broad safety ceilings
  in CI, never brittle wall-time assertions.

Resolution: `scripts/extraction_benchmark.ts` now builds a dedicated release
worker once and runs each extraction input in a separate `/usr/bin/time`
subprocess on macOS or Linux. It accepts an explicit ignored samples directory,
configurable repetitions, and concurrency subsets of 1/2/4. Each JSONL record
contains the input checksum, raw and declared-uncompressed bytes, success/limit/
error/cancelled classification, named limit, wall and user/system CPU time,
peak RSS normalized to bytes, counted serialized output bytes, persisted
artifact bytes, cancellation request and physical drain timing, batch identity,
and a conservative concurrent peak-RSS sum. Machine metadata includes platform,
CPU/memory, toolchain, commit/dirty state, hostname hash, and release-worker
checksum. Extracted content, raw errors, absolute sample paths, and media bytes
never enter results.

The committed sub-500-KiB deterministic corpus covers long ZIP metadata,
repeated PPTX media references, compressed PDF text, and pathological HTML.
Its manifest pins raw sizes and SHA-256 identities, and the generator is covered
by a byte-determinism regression. The pathological HTML also supplies the
post-cancellation drain probe. Local JSONL results and isolated worker artifact
directories are ignored/removed; `data/samples/` remains ignored and is never
an example dependency. The benchmark guide defines the schema, baseline,
comparison rules, conservative concurrency interpretation, and prohibition on
brittle CPU/RSS/wall-time CI assertions.

Focused validation passed three release-worker Rust tests, five Bun parser/
policy/checksum tests, deterministic fixture regeneration, and a 42-record
release subprocess matrix spanning baseline, four fixtures, cancellation, and
concurrency 1/2/4. All seven cancellation workers cancelled and physically
drained. A separate concurrency-1 calibration processed 12 supported local
`data/samples` files: 11 succeeded and `sample-big-dates.xlsx` produced the
expected `IRONFLOW_MAX_XLSX_CELLS` bounded rejection. On this Apple M4 Pro
calibration only, the largest observed sample-process RSS was about 35.3 MB for
`sample-big.xlsx`; this is local trend evidence, not a portable ceiling.
Formatting, the 436-module size policy, `git diff --check`, and
`cargo clippy --all-targets -- -D warnings` pass. The whole integration gate was
not run under the ordinary issue-completion policy.

### IF-068 — Local artifact reads flatten identity into a trusted mutable pathname

**Status:** Resolved on 2026-08-02.

The local artifact store hashes every publication and verifies an existing
content-addressed file before deduplicating into it. Read-side resolution,
however, validates the canonical URI, regular-file type, and descriptor size
without re-hashing the stored bytes. General file consumers then receive a
pathname rather than the opened file handle that was inspected. A process with
write access to `IRONFLOW_ARTIFACT_DIR` can therefore replace an artifact with
same-sized content, or race a consumer between validation and open. Unix mode
hardening reduces accidental writes but does not isolate another workflow or
shell node running under the same OS identity; non-Unix permissions inherit the
configured directory's ACL.

IF-066 documents and preserves this trusted-local-store boundary rather than
claiming that artifact descriptors provide hostile-storage integrity. It is not
a blocker for a deployment that protects the directory from workflow writes,
but it remains P1 for shared or multi-tenant execution.

Required outcome:

- introduce a file-source abstraction so handle-capable consumers never need a
  resolved artifact pathname;
- resolve and open artifacts inside the tracked blocking worker, refuse links
  and non-regular files, hash the opened handle, compare it with the descriptor,
  then rewind and pass that same handle to the parser or decoder;
- provide a guarded verified-path lease only for third-party APIs that cannot
  consume a file handle, and define its cleanup and race guarantees explicitly;
- add same-size replacement, link-swap, cancellation, and cross-platform
  permission regressions;
- document the process-identity and shared-storage isolation requirements, and
  evaluate a separately authenticated artifact service for hostile multi-tenant
  deployments.

Resolution: file inputs now retain a typed raw-path, artifact-descriptor, or
canonical-artifact-URI identity until a tracked blocking worker consumes them.
Artifact reads refuse final links/reparse points on Unix and Windows, require an
opened regular file, hash that opened handle with cancellation checkpoints,
compare the digest and descriptor size, rewind it, and pass that same handle to
the extractor, image/PDF decoder, or transcription reader. HTML, PDF, OOXML,
XLSX, subtitle, image, PDF metadata/render/split, and transcription consumers no
longer flatten artifact identity into the store pathname. Administrative
`resolve` remains explicitly documented as an unauthenticated pathname view,
not a consumer API.

`LocalArtifactStore::verified_path_lease` covers a future third-party path-only
API by copying the verified handle into a private random, size-bounded,
cancellation-aware, read-only temporary file. The lease keeps its handle open
and removes the path on drop, including the Windows read-only cleanup case. No
current built-in consumer needs this weaker path bridge. Documentation now
states that verified handles close the same-size replacement and pathname
TOCTOU gaps but cannot stop a hostile same-OS-identity process from mutating an
already-open inode. Hostile multi-tenant deployments therefore require
separate identities/ACLs or a separately authenticated streaming artifact
service; a shared network filesystem alone does not change the trust boundary.

Regression coverage includes same-size replacement at the store and real
extractor boundary, final-link refusal, pathname replacement after verified
open, cooperative read-verification cancellation, and portable read-only lease
permissions/drop cleanup. Focused validation passed 12 artifact unit tests, two
file-source parser tests, four artifact-handoff integration tests, 21 extractor
integration tests, 17 XLSX integration tests (one existing destructive
reproducer remains ignored), 11 image/PDF integration tests, 28 transcription
tests, and three example-catalog tests. The updated artifact-handoff Lua flow
validates. Formatting, `git diff --check`, the 429-module size policy, and
`cargo clippy --all-targets -- -D warnings` pass. The whole integration gate was
not run under the ordinary-change policy.

### IF-069 — Legacy file materialization and PDF merge amplify before admission

**Status:** Resolved 2026-08-02 (found 2026-07-31).

IF-066 made artifact references the preferred binary handoff and bounded their
extraction, image, PDF-render, metadata, and split consumers. Two older utility
paths still retain the amplification pattern:

- `write_file` decodes a complete Base64 context string into a `Vec<u8>` on the
  async execution path and checks `IRONFLOW_MAX_FILE_BYTES` only afterward. It
  cannot consume an artifact descriptor directly, and append mode limits only
  the new payload rather than the resulting file.
- `pdf_merge` synchronously loads every input into a `Vec<Document>` without a
  file-count, per-file, cumulative-byte, or cumulative-page ceiling. It then
  walks each page separately and can clone the same reachable font, image, or
  resource objects once per page before saving the merged graph. Its `files`
  input accepts paths only, so an artifact must be flattened back to a pathname
  or Base64-adjacent workflow step.

Required outcome:

- preflight Base64 decoded length before allocation and stream decode into a
  bounded destination on a tracked worker; support a mutually exclusive
  artifact source that copies in bounded chunks from the store;
- define overwrite/append final-size behavior, cancellation, partial-file
  cleanup, regular-file, and symlink contracts for `write_file`;
- add PDF merge file-count, per-file, cumulative-byte, page, and retained-object
  ceilings; load inputs sequentially on a tracked worker and remap the union of
  each source's reachable page graph so shared objects are copied once;
- accept canonical artifact descriptors for merge inputs and document local
  store/multi-host requirements;
- add focused malformed/oversized Base64, append-growth, repeated-resource PDF,
  cancellation, artifact-handoff, and partial-output regressions.

Resolution: `write_file` now admits the decoded Base64 length before starting a
tracked blocking worker, streams decoding directly into a staged file, and can
copy a canonical artifact URI or descriptor from the same verified handle used
for its digest and size check. `content`, `source_key`, and `artifact` are
mutually exclusive. Both overwrite and append publish through the shared
rooted staged-file abstraction; append charges the existing bytes plus the new
input against `IRONFLOW_MAX_FILE_BYTES`. Existing append targets must be
regular non-link files. Decode, limit, cancellation, flush/sync, and commit
failures remove staging and preserve an existing destination.

`pdf_merge` now accepts either a static `files` array or a context `source_key`
array containing paths, canonical artifact URIs, or artifact descriptors. It
admits source count before collection, opens and parses sources sequentially on
a tracked blocking worker, and enforces per-file bytes, cumulative input/output
bytes, cumulative pages, and retained graph objects through
`IRONFLOW_MAX_PDF_BYTES`, `IRONFLOW_MAX_PDF_MERGE_FILES`,
`IRONFLOW_MAX_PDF_MERGE_BYTES`, `IRONFLOW_MAX_PDF_MERGE_PAGES`, and
`IRONFLOW_MAX_PDF_MERGE_OBJECTS`. Each source's complete reachable page-object
union is remapped once, so shared fonts, images, and resources are retained once
per source rather than cloned per page. The merged output is staged, bounded
while serializing, synchronized, and atomically committed.

The atomic replacement boundary is handle-relative on Unix. Portable platforms
revalidate the destination immediately before OS-level atomic replacement but
cannot close a hostile parent-directory swap race, so destination trees must be
protected from same-identity mutation. Verified artifact handles remove
pathname re-resolution from these consumers but do not authenticate a hostile
process running under the same OS identity; multi-host execution still requires
the same protected artifact store path or a separately authenticated artifact
service. Artifact retention remains operator-owned.

Focused validation passed two `write_file` unit tests, four file-safety
integration tests, six PDF merge unit tests, one artifact-source PDF merge
integration test, five legacy merge/split integration tests, and three example
catalog tests. All 129 Lua examples validated against the debug binary.
The updated binary round-trip and PDF merge examples also executed successfully
against a disposable local artifact store; the artifact restore matched its
Base64 source and the repeated sample PDF produced a six-page merged output.
Formatting, the 436-module size policy, and
`cargo clippy --all-targets -- -D warnings` pass. The whole integration gate was
not run under the ordinary issue-completion policy.

### IF-070 — `main` requires a check that cannot run before merge

**Status:** Resolved 2026-08-02.

The protected `main` branch required a GitHub Actions status named
`Branch guard`, but repository CI intentionally runs only after pushes to
`main` and `develop`. No pull-request workflow or ruleset emitted that status,
so every release PR was permanently blocked and could only be merged through
an administrator bypass.

The stale required-status-check block was removed from GitHub branch
protection. Pull requests and resolved conversations remain required, while
force pushes and branch deletion remain disabled. The repository CI trigger
contract is unchanged: full CI still runs only on pushes to `main` and
`develop`, plus explicit workflow dispatch. GitHub's branch-protection API now
reports `required_status_checks: null` while the other protections remain
enabled.

### IF-071 — Cancellation cleanup test signals completion before staging cleanup

**Status:** Resolved 2026-08-02.

The `write_file` cancellation regression set its worker-completion flag before
the local `StagedFile` was dropped. A waiter could therefore inspect the
directory during the small interval between the flag becoming visible and the
destructor unlinking the temporary file. This appeared as an intermittent
macOS CI failure with two directory entries even though the production cleanup
path completed correctly.

The test worker now explicitly drops the staged file before publishing its
completion flag. The assertion consequently observes the lifecycle boundary it
claims to test: cancellation has drained the blocking worker, removed staging,
and preserved the original destination. The exact regression passed 100
consecutive focused iterations. Formatting, `git diff --check`, the 436-module
size policy, and `cargo clippy --all-targets -- -D warnings` pass.

### IF-072 — Lease-loss test can race its first heartbeat under load

**Status:** Resolved 2026-08-02.

The executor lease-loss regression waited up to one second for its delay task
to start, but scheduled the first lease heartbeat after only 500 milliseconds.
Under full-suite load, the test's direct lease-file expiry could race a renewal
that had already read the old live lease. That renewal could then replace the
expired fixture with a fresh lease, leaving the five-second delay running past
the test's three-second completion timeout.

The first test heartbeat now starts after two seconds, outside the bounded
one-second task-start window, and the completion budget includes that deliberate
delay. The injected expiry therefore precedes renewal deterministically while
the production heartbeat and lease reconciliation paths remain unchanged.

### IF-073 — A hung evaluation or dead scheduler task silently stops every schedule

**Status:** Resolved 2026-08-02 (found 2026-08-02).

The scheduler has a sound ordinary execution path, but its availability is
coupled to one unsupervised serial task. `scheduler::spawn` returns a
`JoinHandle<()>`; `cmd_serve` stores it in `_scheduler` and only awaits the HTTP
server. A panic or unexpected task exit therefore leaves the API healthy while
all scheduled work has stopped. One evaluation also awaits each schedule claim
and the overlap store reads without a deadline. A backend call that never
returns blocks the only tick loop, including every unrelated schedule.

A claim timeout is not equivalent to a safe claim failure: the backend may
have committed after the caller lost the response, so blindly firing after a
timeout could duplicate a run. The availability repair must preserve this
ambiguity boundary rather than weakening at-most-one admission.

Required outcome:

- tie the scheduler task lifecycle to `serve`, surface unexpected exit through
  process health and structured diagnostics, and shut it down deliberately;
- bound claim and overlap-store operations without treating an ambiguous claim
  timeout as permission to run;
- isolate a stalled schedule so it cannot indefinitely block unrelated names;
- add regressions for a hanging claim, a hanging overlap read, and unexpected
  scheduler task termination while the HTTP server remains otherwise healthy.

Resolution: schedule names now evaluate concurrently, with deterministic
name-sorted results and a 15-second wall-time budget per name. One hanging
claim, overlap read, or flow start therefore cannot hold unrelated schedules.
When the budget expires, every due instant in that schedule's window is
reported as `TimedOut` and its watermark advances through the window. This is
deliberately at-most-once: cancellation cannot prove whether an in-flight claim
or run start committed, so retrying would risk a duplicate.

The tick loop now runs inside a `SchedulerTask` with explicit shutdown. CLI
`serve` supervises that task alongside the API server: normal server completion
requests and awaits scheduler shutdown, while an unexpected return or panic is
logged and returned as a service error, dropping the HTTP server so health
cannot remain green with dead triggers. Focused validation passed three task
lifecycle tests, two hanging-operation isolation tests, 15 scheduler-decision
tests, and 12 scheduler end-to-end tests.

### IF-074 — Schedule configuration and cron semantics are not bounded or portable

**Status:** Resolved 2026-08-02 (found 2026-08-02).

Schedule deserialization validates a non-empty flow, five cron fields, an IANA
timezone, a minimum 60-second grace window, and reserved context keys. It does
not bound schedule count, name length, flow/cron/context size, or maximum grace.
`timing::grace_floor` converts the public `u64` grace to `i64` with `as`, so a
large valid YAML integer can wrap before `chrono::Duration` is constructed.
JSON claims also hex-encode the complete schedule name into a filename, making
an otherwise valid long name fail only when its first instant is claimed.

The catch-up implementation admits at most 64 instants per schedule per tick,
but the CLI contract describes `grace_seconds` as a maximum lateness without
stating that cap. A large grace window can also require scanning many local
minutes before the cap is applied. Finally, the documentation calls the input
"standard five-field cron" while the selected parser requires both restricted
day-of-month and day-of-week fields to match. Traditional crontab semantics use
either field in that case, so an expression such as `0 4 1 * 5` can fire less
often than an operator expects. There is no regression that fixes this joint
day-field contract, and the schedule YAML under `examples/` is only a commented
template rather than a deterministic runnable example.

Required outcome:

- impose reviewed limits on schedule count, names, flow and cron strings,
  context size, and grace; use checked time arithmetic and portable claim keys;
- make the 64-instant catch-up limit and its skip behavior part of the public
  contract, with bounded parsing/catch-up work under hostile configuration;
- either implement traditional day-of-month/day-of-week OR behavior or name
  and document the accepted dialect explicitly, with joint-field regressions;
- provide a deterministic runnable schedule configuration and align its flow,
  README/catalog entry, CLI reference, and validation coverage.

Resolution: `schedules` now deserializes through a bounded map visitor that
rejects more than 256 entries, duplicate/empty names, control characters, and
names longer than 48 UTF-8 bytes before accepting their values. Individual
schedule construction caps flow paths at 1,024 bytes, cron expressions at 256,
timezone names at 128, serialized context at 65,536, and grace at
60..=604,800 seconds. Checked signed conversion replaces the wrapping grace
cast. The schedule-name bound keeps the existing claim filename format within
common component limits, preserving rolling-upgrade claim compatibility rather
than introducing a new hash namespace that old replicas could claim around.

Cron parsing now builds a bounded union when day-of-month and weekday are both
restricted, implementing traditional OR semantics and deduplicating dates that
match both fields. The public contract explicitly states that each tick admits
at most 64 occurrences and skips, rather than queues, the remainder. A new
offline `examples/21-schedules` flow and configuration make the `serve`
scheduler runnable without credentials or external services; the catalog and
example counts include it.

Focused validation passed three bounded-map configuration tests, 16 schedule
configuration/cron tests, nine timezone/catch-up timing tests, 14 default
schedule-claim tests, 15 scheduler-decision tests, 12 scheduler end-to-end
tests, three availability tests, 16 CLI precedence tests, and three example
catalog tests. The runnable configuration claimed and completed one
`scheduled_hello` run against a disposable JSON store with the expected
`_schedule` provenance, and all 130 Lua examples validated. Rustdoc, formatting,
`git diff --check`, the 439-module size policy, and exact all-target Clippy pass.

### IF-075 — Schedule claim cleanup is linear on every fire

**Status:** Resolved 2026-08-02 (found 2026-08-02).

Every JSON and SQL claim first performs best-effort retention cleanup for the
same schedule. JSON streams the complete shared claims directory and stats each
matching entry before atomically creating the new claim. The normal minimum TTL
is seven days, so a minutely schedule retains about 10,080 claims and then scans
that population every minute. SQL likewise performs an unbatched delete before
every insert; the claim table is keyed by `(name, claim_key)` without a cleanup
index ordered by `(name, claimed_micros)`. Redis avoids the scan by assigning a
TTL to each claim key.

The current tests prove cross-replica exclusion and eventual cleanup, including
schedule-specific TTL isolation, but do not exercise the steady-state cost of a
high-frequency schedule with thousands of retained claims.

Required outcome:

- decouple retention from every claim and make cleanup cadence and batch size
  bounded without reopening a duplicate-fire window;
- use a scalable JSON layout/index and add the SQL cleanup index needed by the
  retention predicate;
- retain per-schedule TTL isolation and atomic at-most-one claims under cleanup
  concurrency;
- add a representative 10,000-plus-claim benchmark/regression for JSON and SQL,
  while keeping Redis TTL behavior covered independently.

Resolution: JSON and SQL now admit cleanup through a shared process-local
cadence gate. A schedule runs retention at most once per
`clamp(ttl / 4, 60 seconds, 1 hour)` for each store process and removes no more
than 256 expired claims per pass. The gate retains only SHA-256 schedule
identities and caps its bookkeeping at 1,024 entries; the public scheduler is
already limited to 256 names. A zero TTL remains an explicit immediate-cleanup
contract for deterministic storage tests.

The rolling-upgrade-compatible JSON claim file remains the authoritative atomic
lock. Retention metadata is now stored separately in SHA-256 schedule shards
and hourly buckets, so steady-state cleanup never enumerates another schedule's
claims or live future buckets. Claims written before the index existed are
indexed across all schedule shards in batches of 256, after which one durable
global completion marker removes the transitional flat scan. Index creation and
cleanup remain best-effort: a failure can retain a claim longer but cannot
invalidate its ownership or allow a second winner. SQL now uses a portable
256-row oldest-first delete backed by a covering
`(name, claimed_micros, claim_key)` index. Redis remains independently bounded
by atomic per-key TTL and does not enter the cleanup gate.

Focused validation passed 17 default schedule-claim tests, including 10,001
record JSON and SQLite regressions, bounded legacy migration, schedule-specific
TTL isolation, and same-key races while two stores clean expired claims. The
cadence and existing SQL-prefix-limit unit regressions pass. Live required-test
feature suites passed against disposable PostgreSQL 18.4 (18 tests) and Redis
8.10.0 (19 tests); the Redis TTL regression expired and reclaimed its key
without a sweep. The named containers were removed afterward. Three example
catalog tests and all 130 Lua examples validate. Formatting, `git diff --check`,
the 441-module size policy, and exact all-target Clippy pass. This is focused
local and live-service evidence, not the full integration suite or deployed CI.

### IF-076 — Replica survival has no real-process Docker acceptance gate

**Status:** Resolved 2026-08-02 (found 2026-08-02).

Run ownership is fenced through shared PostgreSQL or Redis leases and expired
owners are reconciled, but the repository has no repeatable acceptance test
that starts multiple real IronFlow processes, routes requests across them, and
kills the owning process. Store-level concurrency tests do not prove that a
surviving replica remains available, sees the same durable state and events,
or terminalizes an abandoned run within the lease budget. They also do not
exercise simultaneous cold-start schema initialization, rolling overlap,
arbitrary container UIDs, or a read-only root filesystem.

Required outcome:

- add an opt-in Docker acceptance environment with two IronFlow replicas, a
  shared durable backend, and a round-robin entry point;
- exercise cross-replica create/read visibility, simultaneous cold start,
  schedule exclusion, SIGTERM and SIGKILL owner death, and bounded lease
  reconciliation with concrete assertions;
- run the image under restricted-container conditions representative of
  OpenShift and document what Docker cannot prove about Railway/OpenShift
  routing and security policy;
- keep the destructive/slow gate outside ordinary focused test execution and
  provide deterministic cleanup of only its explicitly named resources.

Resolution: `deploy/replica/docker-compose.yml` now defines an explicitly named,
opt-in acceptance environment with PostgreSQL 18, two IronFlow processes, and
an nginx round-robin proxy. The IronFlow containers run as arbitrary UID
`1001230000` in root group, with a read-only root filesystem, all capabilities
dropped, `no-new-privileges`, and a `/tmp` tmpfs. The Bun harness refuses to run
without `IRONFLOW_RUN_REPLICA_ACCEPTANCE=1`, creates only the named
`ironflow-replica-acceptance` project, and always removes that project's
containers, network, and disposable volume.

The clean unattended acceptance run proved simultaneous cold start,
round-robin reachability of both replicas, cross-replica durable visibility,
same-key idempotent retry, graceful SIGTERM cancellation and restart, continued
readiness of the survivor after real SIGKILL, production 90-second lease
reconciliation to `stalled`, and one durable run per observed schedule instant.
The harness deliberately retains the production lease duration and is not part
of ordinary focused validation. `docs/REPLICA_DEPLOYMENT.md` records the local
procedure and explicitly limits this evidence: it does not prove Railway edge
routing, OpenShift Route/SCC selection, managed-storage availability, or actual
platform rollout timing.

### IF-077 — Serve lacks draining, readiness, and restricted-container support

**Status:** Resolved 2026-08-02 (found 2026-08-02).

`RunningServer::serve` awaits `axum::serve` without a shutdown signal. The
single `/health` endpoint always returns `ok`, so an orchestrator cannot remove
a terminating or storage-isolated replica before routing new work. API and
scheduler run waiters detach from request/tick ownership and are not registered
with a process lifecycle coordinator, leaving no bounded drain contract.
The runtime image is owned by one fixed user rather than OpenShift's arbitrary
UID/root-group convention, and replica deployments can accidentally select
local JSON/SQLite state or in-memory events.

Required outcome:

- expose distinct liveness and readiness endpoints, with readiness becoming
  false before listener shutdown or run cancellation begins;
- handle SIGTERM/SIGINT through one lifecycle coordinator that stops scheduler
  admission, gracefully stops HTTP admission, tracks accepted runs, waits for
  a bounded drain interval, and cooperatively cancels remaining work;
- add an explicit replica deployment mode that fails closed unless state and
  event backends are shared and durable;
- make the image runnable with an arbitrary non-root UID, dropped capabilities,
  and a read-only root filesystem, and document Railway/OpenShift settings.

Resolution: one shared service lifecycle now owns HTTP and scheduler admission,
accepted-run cancellation authority, and signal-driven draining. SIGTERM and
SIGINT first make readiness fail and stop scheduler admission, then Axum stops
accepting connections while admitted runs receive the configured grace period.
Remaining runs are cooperatively cancelled and given a bounded settling period;
connection shutdown is bounded separately. `IRONFLOW_SHUTDOWN_GRACE_SECONDS`
defaults to 30 seconds, is validated before bind, and is capped at 3,600.

`/health/live` is the process liveness contract (`/health` remains a compatible
alias). `/health/ready` returns success only while the lifecycle admits work and
both state and event stores pass bounded live probes. The explicit
`IRONFLOW_REPLICA_MODE=true` setting fails closed before opening stores unless
both state and event backends are PostgreSQL or Redis. The production image now
uses root-group-writable runtime paths and an arbitrary non-root-compatible
user/group contract; the acceptance environment verifies it with dropped
capabilities and a read-only root filesystem. The deployment guide covers the
required Railway/OpenShift configuration and termination-window boundary.

Focused validation passed two lifecycle tests, including real cooperative
cancellation, the replica-mode fail-closed matrix, 25 API tests, webhook
composition, and the real-process Docker acceptance described by IF-076.

### IF-078 — Retried submissions and schedule claim-to-run gaps can duplicate or lose work

**Status:** Resolved 2026-08-02 (found 2026-08-02).

`POST /flows/run` creates a random run ID after parsing. If a proxy or container
dies after durable creation but before the client receives the response, a
retry starts a second workflow. Schedule admission separately claims an
instant and then loads/starts its flow; owner death in that interval consumes
the claim without leaving a durable run that explains the skipped occurrence.
Automatic replay is unsafe because arbitrary nodes may already have produced
non-idempotent external side effects.

Required outcome:

- accept a bounded `Idempotency-Key` for `/flows/run` and persist its mapping
  atomically with run creation in every shared built-in state backend;
- make same-key/same-request retries return the original run while rejecting a
  key reused for a different admitted request;
- persist a deterministic schedule occurrence identity through run creation so
  retries converge on the same run instead of opening a claim-to-run hole;
- cover concurrent replicas, crash ambiguity, expiration/retention, negative
  key validation, and document that arbitrary workflow execution is fenced and
  terminalized rather than transparently replayed.

Resolution: `/flows/run` now accepts a bounded, validated `Idempotency-Key` of
1-128 ASCII letters, digits, `-`, `_`, `.`, or `:`. IronFlow hashes the key into
a deterministic run identity, never persists the raw key, and stores a
canonical request fingerprint with the run. Same-key/same-request submissions
converge on the existing run across replicas; reusing a key for a different
source or context returns HTTP 409. The mapping intentionally has the same
lifetime as the run record, so deleting the run releases that identity.

Scheduled occurrences now derive a deterministic run ID from the schedule name
and evaluated instant and persist the instant as `_schedule_instant`. A process
that loses or observes the separate claim still converges on that durable run;
atomic run initialization elects the sole executor. This closes owner death
between claim and run creation without replaying already-started arbitrary side
effects. Lease recovery continues to terminalize abandoned execution as
`stalled`; transparent workflow replay remains outside the contract.

Focused validation passed canonical fingerprint and raw-key secrecy unit tests,
the concurrent API idempotency regression (including invalid-key and conflict
cases), 15 scheduler-decision tests, 13 scheduler end-to-end tests including
preclaimed recovery and two-scheduler convergence, three availability tests,
and the live two-process PostgreSQL acceptance. All 132 Lua examples validate;
Rustdoc, formatting, `git diff --check`, the 446-module policy, and exact
all-target Clippy with warnings denied pass. This is focused local and Docker
evidence, not the full integration suite or a Railway/OpenShift deployment.

### IF-079 — Replica lifecycle has no Railway canary evidence

**Status:** Resolved 2026-08-02 (found 2026-08-02).

The Docker acceptance environment proves process and storage behavior under a
local container runtime, but it does not prove Railway's injected port,
deployment healthcheck, private PostgreSQL networking, public routing, or
SIGTERM-to-SIGKILL drain window. A temporary Railway project is available for
an isolated platform canary.

Required outcome:

- deploy two independently addressable IronFlow services sharing one managed
  PostgreSQL backend, with replica mode and readiness healthchecks enabled;
- prove both public endpoints are ready, a run submitted through one is visible
  through the other, and same-key retries converge across services;
- exercise a Railway restart or redeploy while a bounded run is active and
  verify the surviving service remains ready and observes terminal durable
  state within the documented lifecycle boundary;
- record service topology and non-secret operational evidence without treating
  a temporary canary as permanent production monitoring.

Resolution: temporary Railway project
`7d2bad5b-664e-4172-aa7e-898d7e3b0cbe` hosted managed PostgreSQL 18 and two
independently addressable IronFlow services, `ironflow-a` and `ironflow-b`.
Both services ran version `1.16.1-dev.1` with replica mode, PostgreSQL state and
events, generated API authentication, no application volume, and private
Railway reference variables for the database URL. The repository now owns
`railway.json`: Dockerfile build, `/health/ready`, 300-second startup timeout,
always-restart policy, five-second deployment overlap, and 60-second
SIGTERM-to-SIGKILL draining. Railway's live schema requires numeric overlap and
draining values even though its documentation example used strings; the first
config-only failures were superseded by successful deployments
`4ff50f59-1560-4a2d-9978-9d3b84c2f591` and
`ab78c123-7ca1-4794-a591-3e0940539b4f`.

Both public readiness endpoints returned HTTP 200. A run created through A was
read through B as `success`; its same-key retry through B returned the same run
and a different request with that key returned HTTP 409. Railway-native scaling
temporarily ran A with two EU instances: HTTP telemetry attributed 40/40
liveness responses to two distinct deployment-instance IDs. Twenty concurrent
same-key submissions through A's load-balanced domain were distributed across
both instances, all returned HTTP 200, converged on one run ID, and were visible
through B as `success`.

A 300-second delay admitted through A was visible through B as `running` before
a Railway restart. Railway logs recorded readiness drain with one active run,
the configured five-second application grace, cooperative cancellation, and a
new listener on injected port 8080. B observed the durable terminal status as
`cancelled` and remained ready. A was scaled back to one instance after the
native-routing check to limit temporary testbed cost. After the Railway and
OpenShift acceptance work completed, temporary project
`7d2bad5b-664e-4172-aa7e-898d7e3b0cbe` was deleted through the Railway CLI,
stopping `ironflow-a`, `ironflow-b`, and their dedicated PostgreSQL instance and
volume. Railway direct status reports the project as deleted and the project
list retains a deletion timestamp. No secret values were printed or written to
the repository.

This canary proves the tested Railway environment and current deployment
configuration. Railway readiness checks deployment startup rather than
continuous service health, and one temporary project is not evidence for a
different region, plan, storage incident, or long-duration production load.

### IF-080 — Restricted-SCC and Route behavior lack live platform evidence

**Status:** Resolved 2026-08-02 (found 2026-08-02).

Docker exercises an arbitrary UID and restricted container flags, but it does
not run OpenShift admission, assign the namespace UID range, select an SCC,
create a Route, or execute Kubernetes probe and termination behavior. The Red
Hat Developer Sandbox project `gedankrayze-dev` is available for a namespace-
scoped canary without local cluster storage.

Required outcome:

- deploy PostgreSQL and two IronFlow replicas through namespace-owned
  manifests with requests, limits, probes, and a Route;
- run IronFlow under the default restricted SCC without a fixed UID and prove
  the admitted SCC, effective namespace UID, read-only root filesystem, dropped
  capabilities, and non-root execution;
- prove Route readiness, cross-pod durable state and idempotency, graceful pod
  termination, forced owner death, and surviving-replica availability;
- keep secrets out of manifests and output, clean up only named canary
  resources, and document the limits of a shared single-cluster sandbox.

Resolution: `deploy/openshift/canary.yaml` now provides a namespace-scoped
ImageStream and binary BuildConfig, disposable PostgreSQL PVC, two-replica
IronFlow Deployment, Services, edge-TLS Route, probes, resource bounds,
preferred cross-node anti-affinity, and a `minAvailable: 1` disruption budget.
During acceptance, credentials existed only in the generated
`ironflow-canary-secrets` Secret and were neither committed nor printed. An
ImageStream trigger was added after the first deployment showed that pods
created before the binary build could remain in image-pull backoff; the live
trigger resolved the tag to the published digest and completed a
zero-unavailable rollout without a manual restart.

The live acceptance ran in project `gedankrayze-dev` on OpenShift 4.21.21
(Kubernetes 1.34.8). A 3.2 MiB source directory containing only `Cargo.toml`,
`Cargo.lock`, `Dockerfile`, and `src/` avoided uploading the local 58 GiB
`target/`. Build `ironflow-canary-5` compiled IronFlow `1.16.1-dev.1` with the
PostgreSQL feature and published the internal image; the cold build ran from
15:02:56Z to 15:18:30Z, with Cargo's optimized build taking 12m19s. Server-side
dry-run accepted all nine manifest resources before the live application.

Both IronFlow pods were admitted by `restricted-v2` with namespace UID
`1004800000`, `runAsNonRoot`, `RuntimeDefault` seccomp, read-only root mounts,
`allowPrivilegeEscalation: false`, all capabilities dropped, and effective
capability mask zero. The size-limited memory-backed `/tmp` remained writable.
Both replicas became ready on distinct nodes, the edge Route returned the
expected live and ready payloads, and PostgreSQL's PVC was bound through the
project's default storage class.

A flow created directly through one pod completed as `success`, was read from
the other pod, and a same-key retry through that peer returned the same durable
run ID. During a 300-second delay, a normal pod deletion left the peer ready,
cooperatively changed the run from `running` to `cancelled`, and restored two
ready replicas. A second delay was then subjected to forced zero-grace pod
deletion. The peer remained ready, a replacement became ready immediately, and
the abandoned run changed from `running` to `stalled` after about 110 seconds,
inside the documented 90–120 second lease/reaper window. The Route still
returned ready afterward, and the PDB reported two healthy replicas with one
allowed voluntary disruption.

`docs/REPLICA_DEPLOYMENT.md` records the observed contract and limitations;
`deploy/openshift/README.md` documents secret-safe setup, minimal-source build,
admitted-policy inspection, lifecycle checks, and exact cleanup. After the
acceptance, only the named manifest resources, generated Secret, and canary
Build objects were deleted. Exact-name checks found no canary resources; the
project itself and unrelated workloads were not deleted. This is live evidence
for one shared Developer Sandbox cluster, SCC, ingress path, and storage class,
not proof for other operators, quotas, network policies, node failure modes, or
long-duration production availability.

### IF-081 — Hosted canaries lack immutable registry delivery

**Status:** Resolved 2026-08-03 (found 2026-08-03).

The Railway acceptance built the repository independently for each service,
while the OpenShift acceptance uploaded source to a namespace BuildConfig and
deployed an ImageStream `latest` tag. Both platforms ultimately resolved an
image digest, but the operator did not select one published artifact and prove
that exact artifact ran on every replica. Rebuilding the same commit on each
platform also duplicates a long Rust release build and leaves more room for
builder or dependency drift.

Required outcome:

- publish a public `linux/amd64` GHCR image for `develop` and `main` using the
  repository-scoped GitHub token, with a full-commit tag, OCI source/revision
  labels, provenance, and an SBOM;
- make the OpenShift canary reject tags and render only an exact
  `ghcr.io/skitsanos/ironflow@sha256:...` reference;
- prove the resolved Deployment image and every running container image ID use
  the selected digest, while retaining restricted-SCC, readiness, shared-state,
  idempotency, and owner-death behavior;
- document the same digest-only source boundary for Railway without recreating
  a paid temporary testbed merely to repeat already-proven lifecycle behavior;
- remove only the named canary resources and record local, registry, and hosted
  validation evidence separately.

Resolution: `.github/workflows/container.yml` now publishes one public
`linux/amd64` GHCR artifact for source-affecting pushes to `develop` and `main`.
It uses the repository-scoped GitHub token, a full `sha-<40-character-commit>`
tag, OCI source/revision labels, BuildKit provenance, and an SPDX SBOM. It does
not publish `latest` or branch tags. CI now includes the Dockerfile, ignore
file, and container workflow in its own source path boundary. The OpenShift
template no longer contains a BuildConfig, ImageStream, or image trigger;
`deploy/openshift/render.ts` accepts exactly one lowercase
`ghcr.io/skitsanos/ironflow@sha256:...` reference and rejects tags, foreign
repositories, malformed digests, and missing or duplicate markers.

Local validation was separated from registry and hosted evidence. A clean
`linux/amd64` Docker build completed from the prepublication candidate and its
runtime passed the non-root/read-only packaging smoke check. After the final
`1.16.1-dev.3` bump, the complete local integration gate passed: policies,
module and skill checks, formatting, default and feature Clippy, default and
Redis/PostgreSQL tests, the release build, and all 132 Lua examples. The first
attempt exhausted the 89 GiB Cargo build cache rather than failing a test;
`cargo clean` removed only recoverable build artifacts, and the clean rerun
passed.

Container workflow run
`https://github.com/skitsanos/ironflow/actions/runs/30790340800` published commit
`6694f091cb6fdd30690e5c22ada52b01b0ec756f`. Anonymous GHCR manifest access
returned HTTP 200. The attested OCI index is
`sha256:57be20c2f9468be7a56efe9f2b8c0fbf5560c1b7af17f517528aadbcef7680e3`;
it contains one runnable `linux/amd64` manifest,
`sha256:435a3c3ab154b9ca0454d2eac91275e8708204c18cded7e18a68cf74658f261f`,
plus SPDX and SLSA provenance attestations. Deployment documentation selects
that runnable child explicitly so the declared image and runtime `imageID`
have the same digest instead of confusing the attested index with its platform
manifest.

The runnable digest was exercised in the Red Hat Developer Sandbox project
`gedankrayze-dev` on OpenShift 4.21.21. The Deployment image and both container
`imageID` values matched it exactly. Both replicas were ready under
`restricted-v2`, namespace UID `1004800000`, zero effective capabilities, and
a read-only root mount; the Route returned the `1.16.1-dev.3` ready payload.
A run created directly through one pod was visible through its peer. A
same-key retry converged on the single deterministic run and a different
payload returned HTTP 409. After immediate zero-grace deletion of the owner,
the peer and Route stayed ready, the replacement used the same digest, and the
abandoned run became `stalled` after 94 seconds, within the documented
90–120-second window.

Railway now documents the same public digest-only source contract without
recreating the deleted paid testbed. The OpenShift test used generated Secret
values without printing or committing them. After each live pass, exact-name
and label checks confirmed that the two Deployments, two Services, Route, PDB,
PVC, Secret, and canary pods were gone; the namespace and unrelated resources
were not removed.

### IF-082 — Repeated integration gates retain stale linked artifacts

**Status:** Resolved 2026-08-03 (found 2026-08-03).

The local integration gate reuses the repository `target/` directory across
default, feature-enabled, and release builds. A clean complete gate occupied
about 50 GiB, including 42 GiB in `target/debug/deps` and 5.9 GiB of incremental
state. After the package version changed, Cargo retained the previous linked
integration-test executables beside the new set. The directory reached 85 GiB
and the macOS linker failed with `errno=28` before any test assertion ran.

Required outcome:

- keep default, feature-enabled, release, Lua, and live-storage coverage intact;
- prevent repeated versioned gates from retaining old IronFlow binaries and
  hundreds of statically linked integration-test executables;
- preserve reusable third-party dependency artifacts rather than applying an
  unrestricted `cargo clean` to the entire target directory;
- remove gate-generated package artifacts on success, failure, or interruption,
  and disclose that user-visible cleanup in repository instructions;
- add a regression that prevents the package-scoped cleanup, exit cleanup, or
  incremental-build boundary from being removed accidentally.

Resolution: `scripts/integration_gate.sh` now registers cleanup before its
command preflight, package-cleans only `ironflow` artifacts before validation,
sets `CARGO_INCREMENTAL=0` for every Cargo phase, and repeats the same
package-scoped cleanup from an `EXIT` trap. SIGINT and SIGTERM terminate through
that exit path. Disposable Redis/PostgreSQL cleanup shares the same handler and
remains limited to the exact per-process container names. Default,
feature-enabled, release, Lua-example, audit, and live-storage commands remain
in the gate unchanged.

The package-scoped boundary was measured against the completed IF-081 gate
cache. `target/` started at 52,219,332 KiB (about 50 GiB), and
`cargo clean --package ironflow` removed 41,544 files / 43.6 GiB, leaving
8,514,492 KiB (about 8.1 GiB) of reusable dependency and build artifacts. A
subsequent `CARGO_INCREMENTAL=0 cargo check --all-targets` produced zero KiB in
`target/debug/incremental`; the same package cleanup removed its 11.2 MiB of
workspace output and restored the dependency-only baseline.

Repository policy coverage now rejects unrestricted or workspace-wide Cargo
cleanup, requires the pre-gate prune, disabled incremental output, and exit
cleanup, and executes the gate with a fake toolchain to prove that a failing
first Cargo command still produces the exact sequence `package clean -> work ->
package clean` while preserving the command's exit code. `AGENTS.md` and the
repository `check-ironflow` skill disclose that a full local gate intentionally
removes IronFlow binaries and linked tests while retaining dependency caches.

### IF-083 — Container dependency cache is invalidated by routine changes

**Status:** Resolved 2026-08-03 (found 2026-08-03).

The container builder copied `Cargo.toml`, `Cargo.lock`, and all production
source before running one release build. Any source edit invalidated the entire
compilation layer. The required development-version bump also changes the root
package version in both Cargo files immediately before each `develop` push, so
even a metadata-only candidate forced every third-party dependency to compile
again. The external GitHub Actions cache could retain the resulting layer but
could not reuse it when either input checksum changed.

Required outcome:

- separate dependency compilation from application compilation without a
  manually maintained shadow manifest;
- keep the dependency cache stable across source-only and local package-version
  changes while invalidating it for dependency, feature, lockfile, builder, or
  toolchain changes;
- pin the Rust and dependency-planner versions used by the container build;
- preserve the `postgres redis` production feature set and locked dependency
  resolution;
- isolate the exported BuildKit cache under an image-specific scope and prove a
  version-only rebuild reuses the dependency layer.

Resolution: the Dockerfile now uses the version- and manifest-digest-pinned
`lukemathwalker/cargo-chef:0.1.77-rust-1.97.1-bookworm` image for planner and
builder stages. `cargo chef prepare` derives the dependency recipe from the
real manifests and target layout while masking local crate versions. The
builder cooks that recipe with `--release --locked`, the production feature
set, and the `ironflow` binary target before copying the real source for the
application-only build. Dependency and source changes therefore remain
separate without duplicating Cargo metadata. The container workflow exports
and imports the layer using the dedicated `ironflow-container-amd64` GHA cache
scope.

A clean local BuildKit builder-stage probe compiled the dependency recipe in
92.8 seconds and IronFlow itself in 32.8 seconds. An isolated second context
changed only the root version from `1.16.1-dev.4` to `9.9.9-dev.9` in
`Cargo.toml` and `Cargo.lock`: BuildKit reported both the recipe-copy and
`cargo chef cook` steps as cached, then rebuilt only IronFlow in 38.4 seconds.
A complete runtime image built from the normal context and returned
`ironflow 1.16.1-dev.4`. This proves the local cache boundary; the first pushed
workflow run will be the separate hosted-GHA cache observation.

### IF-084 — Example validation serializes and recompiles CI work

**Status:** Resolved 2026-08-03 (found 2026-08-03).

The `validate-examples` job depended on the complete two-platform `build`
matrix, so it could not start until the slower macOS build completed. It then
ran `cargo build --release` again on Linux even though the matrix had already
produced the exact binary it needed. Finally, it reran the catalog, contract,
and fixture integration tests already covered by the independent default
`cargo test --all-targets` jobs.

Required outcome:

- retain independent Linux and macOS release-build evidence and their existing
  human-readable check names;
- make example validation depend only on the Linux binary it executes;
- transfer that exact commit-scoped binary between jobs with short retention
  and fail if the producer did not create it;
- preserve validation of every Lua file while leaving Rust example contract
  tests under the existing default all-target test jobs;
- add policy coverage that rejects a second build or test compilation in the
  example-validation job.

Resolution: the former build matrix is split into `build-linux` and
`build-macos`, retaining check names `Build (ubuntu-latest)` and
`Build (macos-latest)`. Linux uploads
`ironflow-linux-release-${{ github.sha }}` with one-day retention and
fail-on-missing behavior. `validate-examples` depends only on `build-linux`,
downloads that artifact, restores its executable bit, and runs every Lua flow
through it. It no longer installs Rust, builds a second release binary, waits
for macOS, or repeats the three example integration-test targets; those remain
included in both default runs of `cargo test --all-targets`.

Focused repository-policy tests parse the workflows and require the split-job
dependency, upload/download handoff, Lua invocation, and absence of Cargo build
or test commands in example validation. All seven policy tests passed,
`actionlint .github/workflows/*.yml` passed, and `git diff --check` passed. No
Lua behavior, node contract, or example content changed, so the existing 132
flows were not revalidated locally for this workflow-only scheduling change.

### IF-085 — Hosted CI and container-cache acceptance is missing

**Status:** Resolved 2026-08-03 (found 2026-08-03).

IF-083, IF-084, and IF-086 establish local and static evidence for dependency
layer reuse, release-binary handoff, and bounded Rust compilation jobs. None of
that proves GitHub-hosted runners import the new BuildKit cache, preserve the
artifact handoff, or improve the real workflow critical path. The latest green
baseline before those changes is CI run `30793338123`, which completed in
25 minutes 38 seconds and spent about 51 runner-minutes across seven Linux Rust
compilation jobs. Container run `30793340800` took 13 minutes 52 seconds with
the old single compilation layer.

Required outcome:

- publish the accumulated IF-082 through IF-086 candidate only after incoming
  PR, remote ancestry, clean-worktree, development-version, and complete local
  integration gates pass;
- require green hosted CI and container publication for that exact commit;
- verify Lua validation downloads the Linux build artifact and does not compile
  Rust or wait for the macOS build;
- record hosted job durations and compare the Rust-compiling job count and CI
  critical path with the baseline without overstating results;
- publish a second version-only candidate and verify the hosted container build
  imports the dedicated cache and reports the cargo-chef dependency cook as
  cached;
- record immutable image digests and distinguish local, first-run hosted, and
  warm-cache hosted evidence.

First hosted pass: commit `463bba63eb1953314506252383e58feb6b89476f`
(`1.16.1-dev.5`) passed CI run `30806165397` in 12 minutes 51 seconds,
compared with 25 minutes 38 seconds for baseline run `30793338123`—about a
50% shorter critical path. Summed job time fell from 91 minutes 48 seconds to
52 minutes 44 seconds, about 43% less hosted runner usage. The consolidated
feature/storage job was the new critical path at 12 minutes 45 seconds. Lua
validation downloaded artifact `ironflow-linux-release-463bba6...`, validated
all 132 flows in a seven-second job, and finished before the macOS build; its
log contains no Cargo build or test command.

Cold container run `30806165551` passed in 13 minutes 5 seconds versus 13
minutes 52 seconds for the old-layer baseline `30793340800`. As expected, the
cargo-chef dependency cook compiled from cold state and exported the new scoped
cache. It published attested OCI index
`sha256:a3df7135ede24cb3c2896954af5d4c22bb242324e28e7fd7a4c5729b06d13be9`.
This first pass proves workflow correctness and cache population, not warm
dependency reuse.

Warm hosted pass: the version-only follow-up commit
`fc431dcf636b826d674e2d22cf3b6a3e9cd498d1` (`1.16.1-dev.6`) passed CI run
`30808017710` in 12 minutes 12 seconds. Its five Rust-compiling jobs remained
bounded to default Linux validation, combined feature/storage validation,
Linux and macOS release builds, and macOS tests. Their durations were 10
minutes 14 seconds, 12 minutes 7 seconds, 8 minutes 50 seconds, 11 minutes 37
seconds, and 9 minutes 42 seconds respectively. Total hosted job time was 53
minutes 27 seconds, within 43 seconds of the first pass and still about 42%
below the 91-minute-48-second baseline. Linux artifact handoff remained
independent of macOS: the Linux build completed first, and the downstream
example job validated all 132 flows in 10 seconds without a Cargo command.

Warm container run `30808017823` passed in 4 minutes 15 seconds, 8 minutes 50
seconds (about 68%) faster than the cold cargo-chef run. The BuildKit log
explicitly reports the `cargo chef cook --release --locked --features
"postgres redis"` step as `CACHED`; importing and extracting its 755.54 MB
dependency layer took 59 seconds and no dependency compilation occurred. The
run published commit tag
`sha-fc431dcf636b826d674e2d22cf3b6a3e9cd498d1` at attested OCI index
`sha256:7334dc0630d8791d2db3825b48194ce53e71cf74ad6aa4114dbc6780573cc358`.
An independent `docker buildx imagetools inspect` resolved that exact immutable
reference and confirmed its Linux/amd64 image plus attestation manifest. This
separates the complete local pre-push gate, cold hosted cache population, and
warm hosted cache reuse and closes the acceptance gap.

### IF-086 — CI repeatedly compiles the same Rust graph

**Status:** Resolved 2026-08-03 (found 2026-08-03).

The CI workflow isolated default `check`, default Clippy, full-feature check and
Clippy, Redis tests, PostgreSQL tests, default Ubuntu tests, and the Linux
release build into seven independent Rust workspaces. `rust-cache` deliberately
keeps `cache-targets: false`, so each runner reused downloaded dependencies but
not compiled target artifacts. The latest completed `develop` run before this
change spent about 51 runner-minutes across those seven Linux compilation jobs;
the Redis and PostgreSQL jobs alone ran for 9 minutes 48 seconds and 14 minutes
58 seconds respectively.

Required outcome:

- retain exact default and `postgres,redis` Clippy coverage, default and
  feature-enabled all-target tests, required live Redis/PostgreSQL execution,
  doctests, Linux/macOS tests and release builds, and Lua validation;
- reuse one workspace within related default and feature-enabled commands
  instead of transferring a large mutable `target/` directory between runners;
- run Redis and PostgreSQL tests together using the combined feature contract
  already exercised by the local integration gate;
- remove standalone `cargo check` passes already subsumed by exact all-target
  Clippy invocations;
- preserve the macOS test/build and Linux build check names, release artifact
  handoff, generated storage credentials, and fail-closed required-test flags;
- add a regression that bounds the set of Rust-compiling CI jobs.

Resolution: default Linux Clippy, all-target tests, and doctests now share the
`rust-default` job. Full-feature Clippy and all-target tests now share the
`rust-features` job, which starts both Redis and PostgreSQL services and sets
both required-test flags. This replaces the standalone check, full-feature,
and per-backend jobs without weakening their commands. macOS keeps an
independent `Test (macos-latest)` job, while both release-build jobs and the
Linux artifact handoff from IF-084 remain unchanged. The workflow therefore
has three Linux compilation boundaries—default, combined features/storage, and
release—instead of seven. It intentionally does not exchange `target/`
artifacts between runners because those artifacts are large, profile-specific,
and responsible for the local growth addressed by IF-082.

Repository-policy coverage rejects all six retired job IDs, requires the exact
default Clippy/test/doctest commands, requires both services and required-test
flags on the feature job, rejects a redundant feature `cargo check`, and bounds
Rust compilation to the two Linux validation jobs, two release jobs, and the
macOS test job. All eight focused policy tests and
`actionlint .github/workflows/*.yml` passed. The exact combined feature Clippy
command passed locally, followed by the complete `postgres,redis` all-target
suite against disposable Redis and PostgreSQL containers: 411 library tests
passed with one ignored, and every integration/example target passed. The two
named containers were removed afterward, and package-scoped cleanup removed
1,848 IronFlow artifacts / 17.9 GiB while retaining dependency caches. Hosted
runner-time improvement remains an observation for the next pushed CI run; it
is not claimed by this local validation.

### IF-087 — Warm container cache transfer is oversized

**Status:** Resolved 2026-08-03 (found 2026-08-03).

IF-085 proved that cargo-chef avoids dependency recompilation, but GitHub's
fresh runner still downloaded and extracted a 755,538,199-byte dependency
layer. The GitHub Actions cache backend accepts `min` or `max` mode but exposes
no compression control. Changing to `mode=min` would omit the intermediate
cargo-chef layer and silently discard the reuse IF-083 established.

Required outcome:

- retain `mode=max`, the separate dependency cook, pinned builder, locked
  feature-complete release build, immutable application tags, SBOM, and
  provenance;
- use a cache backend and OCI representation that supports explicit zstd
  compression without turning the mutable cache tag into a deployment
  reference;
- keep cache reads and writes within the existing GHCR package boundary and
  require policy coverage for the backend, cache reference, compression, and
  removal of the former GHA exporter;
- compare gzip and zstd exports of the same complete cached graph, including
  the dependency-layer byte count and compression-time trade-off;
- distinguish controlled local export evidence from hosted transfer evidence,
  which can only be collected after the next authorized `develop` push.

Resolution: container publication now imports and exports a dedicated
`ghcr.io/skitsanos/ironflow:buildcache-amd64` registry cache. It retains
`mode=max` so intermediate cargo-chef results remain available and emits an OCI
image manifest with forced zstd level-15 compression. The existing GHCR login
and package-write permission cover this cache; the commit-tagged application
image, immutable digest, SBOM, and maximum provenance settings are unchanged.
README, deployment, and implementation-plan documentation distinguish the
mutable cache manifest from runnable deployment references. Repository-policy
coverage requires the registry reference and every compression/OCI setting and
rejects reintroduction of a GHA cache exporter.

A controlled Docker Buildx benchmark compiled the graph once, then exported
the same complete cache with forced gzip level 6 and forced zstd level 15.
Total cache bytes fell from 1,434,765,316 to 991,163,277 (30.9%); the
cargo-chef dependency layer fell from 755,538,199 to 461,494,335 bytes (38.9%).
The gzip export took 52.8 seconds and zstd took 63.0 seconds locally, accepting
about 10 seconds of producer CPU to avoid roughly 294 MB on each cold-runner
dependency restore. Both builds used the same cached Dockerfile graph and the
second reported every build step cached. The temporary 2.4 GB export directory
was moved to the system Trash after measurement; reusable Docker build state
was retained.

All eight focused repository-policy tests, the full workflow `actionlint`
check, and `git diff --check` passed. This is local
OCI-export and static workflow evidence, not a claim that GHCR has accepted the
new cache manifest or that a hosted runner has imported it. Those observations
belong to the next authorized `develop` publication before release promotion.

Hosted acceptance completed on 2026-08-03. Candidate
`3c756831edfb67ea944647869d32c6630fc8d986` (`1.16.1-dev.7`) passed CI run
`30815166883` in 13 minutes 14 seconds and populated the previously absent
registry cache in Container run `30815167729`. GHCR accepted
`buildcache-amd64` as an OCI image manifest at
`sha256:206781b52f82bd15e9db88b0e700ea8440b15ed3fbdec0faf02d0d597ad8223d`;
the cold migration completed in 13 minutes 28 seconds and published the
attested application index
`sha256:f6afec1b1026f9fd678f2c8571f10359c2a6cc3c64acebec138c5d2fc7a19879`.

A manual fresh-runner rerun of that exact commit, Container run `30816243328`,
completed in 34 seconds. BuildKit imported the registry manifest in 0.5 seconds,
reported the cargo-chef cook and every application/runtime build step as
`CACHED`, spent 0.3 seconds preparing the unchanged cache export, and completed
the build/publish step in 12 seconds. This exact-graph rerun is the hosted warm
cache proof; it does not claim that a changed dependency graph avoids its
required rebuild.

The final `1.16.1-dev.8` candidate integrated Renovate PR 115's Lettre 0.11.23
lockfile update after 12 focused email-node tests and `cargo audit` passed. Its
complete local pre-push gate and hosted CI run `30817537738` passed, including
all 132 Lua examples and required disposable Redis/PostgreSQL suites. Container
run `30817538258` imported the prior cache manifest, rebuilt the changed
dependency graph, and replaced it with zstd OCI cache manifest
`sha256:42c9bf597234b30420ed0308543a6bfa49cdb01ed873f6f6f17b67e2388f4087`.
It published attested application index
`sha256:11325f78399470d14044732e38fd42779b1a85708f53191533c8f31f0ca7fd5f`.
This closes both the hosted registry-compatibility and warm-reuse boundaries.

### IF-088 — Detached run-admission cleanup has a tight CI deadline

**Status:** Resolved 2026-08-03 (found 2026-08-03).

The stable `1.16.1` merge passed every `main` CI job except the combined
feature/storage suite. Run `30820621471`, job `91709337732`, failed in
`a_blocked_parse_makes_a_concurrent_validation_fail_fast` after the durable run
had already reached `Success`: the test allowed one additional second for the
detached coordinator to stop its lease heartbeat and release process admission.
That cleanup is intentionally ordered after terminal status persistence, so a
loaded hosted runner can observe the terminal record before the supervisor has
finished even though admission is released correctly.

Required outcome:

- retain the one-second fail-fast checks for contested flow-load and run
  admission;
- continue proving that a disconnected request cannot release its run permit
  before physical completion;
- bound the post-terminal cleanup wait without treating durable terminal status
  as synchronous supervisor completion;
- change no production, public API, node, documentation, or Lua-example
  contract for a test-only timing correction.

Resolution: only the final post-`Success` permit-restoration observation window
was increased from one to three seconds, matching the existing durable-status
wait used by this regression. The one-second contention/refusal deadlines are
unchanged. This preserves detection of a stranded permit while allowing the
detached heartbeat supervisor to finish under hosted-runner scheduling load.

Before the change, the exact feature-enabled regression passed five consecutive
local runs, confirming the hosted failure was timing-sensitive rather than a
reproducible permit leak. After the change it passed ten consecutive runs with
`postgres,redis` enabled. `cargo fmt --all -- --check`,
`python3 -B scripts/check_module_size.py`, and
`cargo clippy --all-targets -- -D warnings` also passed. Corrective PR 117
merged as `febc88b9ea0af3fd91480d017260be12dec5dcc7`; full `main` CI run
`30822724617` then passed in 13 minutes 14 seconds. Its combined feature and
live-storage job passed in 12 minutes 32 seconds, including the previously
failing regression against required disposable Redis and PostgreSQL services.
This closes the hosted boundary; release tagging remains a separate approval.

### IF-089 — Tag-scoped Windows caches force both release variants to rebuild dependencies

**Status:** In progress (found 2026-08-03; implementation complete locally,
hosted acceptance pending).

The `v1.16.1` release workflow run `30831343650` completed in 21 minutes 6
seconds and was gated by Windows. The default Windows job took 20 minutes 20
seconds, including an 18-minute-45-second release build; the full-feature job
took 18 minutes 14 seconds, including a 17-minute-8-second release build. Both
`Swatinem/rust-cache` steps reported `No cache found` and then compiled the
same broad dependency graph independently.

The previous `v1.16.0` tag had populated the corresponding caches one day
earlier, but those entries belonged to a different tag ref. GitHub's
[cache access restrictions](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#restrictions-for-accessing-a-cache)
let tag workflows restore caches from the default branch, not sibling tag
refs, so changing dependency hashes made every release tag a cold Windows
build despite the cache action being present.

Required outcome:

- prime the default and `postgres,redis` Windows release dependency graphs on
  the exact stable `main` commit after it enters the required CI boundary;
- store both graphs under one default-branch cache key that both parallel
  Windows tag jobs can restore;
- keep that tag-side cache use read-only so parallel variants do not race to
  save the same entry, while retaining a correct cold-build fallback;
- never reuse a prebuilt IronFlow application binary: each default/full archive
  must still be compiled and packaged from the tagged source;
- retain every release target, archive name, feature boundary, checksum, and
  main-ancestry guard;
- prove the policy statically and then record cache restoration and job timing
  from the next hosted `main` plus tag workflow pair.

Implementation: `main` CI now has one Windows-only release-cache primer. It
builds the default and full release graphs sequentially under
`release-x86_64-pc-windows-msvc`; `rust-cache` removes the workspace crate
before saving, so the entry contains reusable dependency artifacts rather than
an IronFlow binary. Both Windows rows in the release matrix remain parallel,
use the same key and compiler flags, restore the cache read-only, and compile
their own exact-tag default or full binary. A missing cache therefore affects
duration only. Linux and macOS retain independent per-target/per-feature cache
keys and their existing artifacts.

The CI path filter now includes the release workflow, and repository-policy
coverage parses both workflows to require the main-only primer, the two build
commands, absence of application-artifact upload, one shared Windows key, two
read-only tag variants, and unchanged default/full suffixes. Release guidance
now requires the exact `main` primer to pass and records cache restoration as a
performance acceptance gate. No Rust runtime, node, documentation contract, or
Lua example changed, so node pages and examples do not require edits for this
workflow-only issue.

Focused local validation passed `bun test
scripts/tests/repository_policy.test.ts` (9 tests, 77 expectations),
`actionlint .github/workflows/*.yml`, `bun run scripts/validate_skills.ts` (3
repository skills), and `git diff --check`. The complete integration gate had
already passed for the containing `1.16.2-dev.1` post-release batch before this
workflow-only change; it is not represented as validation of IF-089. IF-089
remains open until the next exact-commit `main` run primes the cache and the
following tag run proves both Windows restores and records hosted durations.

### IF-090 — Dependency warnings are not enforced as a shrinking budget

**Status:** Resolved on 2026-08-04. The one exception present at completion was
subsequently removed by IF-091.

`cargo audit` reported five informational advisories but exited successfully.
The highest-risk warning was `RUSTSEC-2026-0221`: `event-listener 5.4.1`
unsafely allowed a non-`Send` tag to cross thread boundaries, and both SQLx and
Redis reached that version. The other warnings were abandoned transitive
crates retained by broad defaults: `bincode` and `yaml-rust` through Comrak's
unused CLI/syntax-highlighting features, `paste` through unused AVIF/OpenEXR
image codecs, and `ttf-parser` through `pdf-extract`'s `lopdf 0.42` pin.

Required outcome:

- move `event-listener` to the patched `5.4.2` release without replacing SQLx
  or Redis;
- remove unused dependency features rather than ignoring their advisories;
- keep the image-node decoder surface explicit and covered by a regression;
- make every future audit warning fail CI and the integration gate unless its
  advisory has a narrow, reviewed exception with an upstream removal trigger;
- preserve Markdown conversion, image nodes, PDF extraction, SQLite,
  PostgreSQL, and Redis behavior.

Resolution: the lockfile now resolves `event-listener 5.4.2`. Comrak runs as
the parser library IronFlow actually uses, without its CLI and syntax-highlighter
defaults. Image decoding explicitly enables BMP, Farbfeld, GIF, HDR, ICO, JPEG,
PNG, PNM, QOI, TGA, TIFF, and WebP; AVIF encoding and OpenEXR were never part of
the documented output contract, and removing them plus an unused `lopdf`
`embed_image` feature removes the `paste` graph. Node documentation and a
codec-surface regression record this boundary. These changes remove four of
the five warnings and 70 transitive packages from the lockfile.

At IF-090 completion, `RUSTSEC-2026-0192` remained isolated to
`pdf-extract 0.12 -> lopdf 0.42 -> ttf-parser 0.25.1` because upstream still
pinned the older `lopdf`. IF-091 subsequently removed the redundant parser,
the legacy dependency chain, and this temporary audit exception.

`cargo audit --deny warnings` is now authoritative in CI, the local integration
gate, repository guidance, and the validation skill. Repository-policy coverage
fails if that strict command is weakened, the four removed packages return, or
the patched listener regresses. Validation evidence is recorded with the
implementing change: `cargo audit --deny warnings`; 10 repository-policy tests
(87 expectations); validation of all three repository skills; `actionlint`;
9 focused image tests; the focused Markdown and PDF-extraction tests; default
and `postgres,redis` all-target checks/strict Clippy; formatting and the
446-module size policy; and static validation of both affected Lua examples
all passed. The local tests exercise SQLite plus the combined dependency graph;
live Redis/PostgreSQL behavior was not changed and was not rerun for this
dependency-only task.

### IF-091 — PDF extraction duplicates parsers and retains the final advisory

**Status:** Resolved on 2026-08-04.

`extract_pdf` loaded every input into `lopdf 0.44` for page and metadata checks,
then dropped that document and gave the complete byte buffer to `pdf-extract`
for a second parse. `pdf-extract 0.12` pinned `lopdf 0.42`, which retained the
unused `ttf-parser` advisory and a second PDF dependency graph. Its extracted
text was fully materialized before IronFlow could enforce
`IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES`.

Required outcome:

- parse each PDF once while preserving text, Markdown, metadata, page limits,
  regular-file handling, and cancellation behavior;
- bound per-page decompressed content, font-mapping streams, and aggregate raw
  text before they can exceed the extraction-output budget;
- remove `pdf-extract`, the old `lopdf`, `ttf-parser`, and the associated audit
  exception without adding a replacement dependency;
- align the node documentation and runnable Lua example with the resource
  contract and prove the dependency boundary in repository-policy coverage.

Resolution: `extract_pdf` now retains its initial `lopdf 0.44` document and
releases the input byte buffer after parsing. It extracts one page at a time
with `extract_text_with_limit`; each call receives only the unspent aggregate
text budget, so previously accumulated text plus the next page's decompressed
content and ToUnicode streams cannot cross
`IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES`. Cancellation checkpoints bracket each
opaque page call, and the existing item and final serialized-output checks
remain authoritative. Focused text/Markdown content markers, metadata, and the
three-page fixture remain compatible. IF-092 subsequently recorded matching
non-whitespace character sequences across three PDFs and the explicit
best-effort whitespace boundary for positioned glyphs.

Removing `pdf-extract` leaves one `lopdf` version and removes its legacy parser
chain, including `ttf-parser`, without a replacement dependency. The lockfile
now contains 545 packages, 16 fewer than the IF-090 candidate, and strict audit
passes without the `RUSTSEC-2026-0192` exception. Repository-policy coverage
requires that the removed packages stay absent and that only one `lopdf`
package is locked. The node page, Lua example, and example catalog now describe
the bounded, single-parse contract.

Focused validation passed: PDF text/Markdown/metadata and missing-file tests
(2 tests); PDF resource, page, item, malformed-metadata, regular-file, symlink,
FIFO, and page-decompression boundaries (1 test); extraction cancellation and
deadline tests (2 tests); the clean-checkout fixture matrix (2 tests); the
repository-policy suite (10 tests, 90 expectations); static validation and a
successful runtime execution of `examples/08-extraction/extract_pdf.lua`; and
a negative runtime probe that failed on page 1 at a 64-byte decompression
budget. `cargo audit --deny warnings`, `cargo fmt --all -- --check`, the
446-module size policy, exact strict Clippy, and `git diff --check` also passed.
No PostgreSQL, Redis, live-service, or full integration gate was run because
this change does not touch storage and is not an integration-boundary push.

### IF-092 — PDF resource improvements lack matched RSS and timing evidence

**Status:** Resolved on 2026-08-04.

IF-091 removed a duplicate PDF parser and bounded page decompression, but its
acceptance evidence proves correctness and dependency reduction rather than the
actual process-memory and execution-time shape. IF-067 provides a safe release
subprocess harness, although it cannot select only PDF workloads without
constructing temporary fixture directories or running unrelated extractors.

Required outcome:

- add a validated PDF-only selector to the existing extraction benchmark while
  preserving its empty-process baseline and content-safe JSONL schema;
- run the same deterministic compressed-text fixture and representative local
  PDF samples against the pre-IF-091 and current workers on the same machine;
- use at least three repetitions at concurrency 1, 2, and 4, report medians and
  absolute peak-RSS values, and distinguish process peaks from simultaneous
  host memory;
- document commands, input checksums, environment, and interpretation without
  turning machine-specific timing or RSS observations into brittle CI limits.

Implementation: the existing benchmark accepts `--nodes` with a validated,
deduplicated comma-separated extractor list. Filtering applies equally to the
committed fixtures and explicit local samples while the empty-process baseline
always remains. Schema version 2 adds `worker_elapsed_ms`, measured
monotonically from immediately before node setup/execution through blocking
runtime drain. This gives sub-centisecond operations meaningful resolution
without discarding the complete-process `/usr/bin/time` fields. A regression
keeps the Bun orchestrator and Rust worker schema constants aligned, and the
benchmark guide documents selection, timing interpretation, and matched-worker
comparison rules.

Matched calibration used the measurement-identical schema-2 worker with
production commits `dd501ff` (before IF-091) and `a7419c1` (IF-091), release
mode, an Apple M4 Pro with 12 logical CPUs and 24 GiB RAM, Bun 1.3.14, and Rust
1.97.1. Each side produced 84 successful observations: three repetitions of
the empty baseline plus three checksum-identical PDFs at concurrency 1, 2, and
4. JSONL remained outside the repository and contained no extracted content or
raw errors.

Concurrency-1 medians, with the concurrency-4 column showing the conservative
sum of four independent process peaks, were:

| Input | Raw bytes | Peak RSS before -> current | Worker ms before -> current | Four-process peak sum before -> current |
|---|---:|---:|---:|---:|
| `compressed-text.pdf` (`7406ee5d...`) | 8,164 | 13.53 -> 12.22 MiB (-9.7%) | 58.876 -> 1.909 (-96.8%) | 54.69 -> 49.06 MiB |
| `generated_book.pdf` (`49084cfa...`) | 1,840,506 | 16.70 -> 14.56 MiB (-12.8%) | 0.676 -> 0.575 (-15.0%) | 66.77 -> 58.27 MiB |
| `sample.pdf` (`85e68c36...`) | 108,505 | 15.88 -> 13.34 MiB (-15.9%) | 2.777 -> 1.565 (-43.7%) | 62.16 -> 53.41 MiB |

The current empty baseline was 9.08 MiB versus 9.80 MiB before IF-091. After
baseline adjustment, the three concurrency-1 PDF increments fell by roughly
16%, 21%, and 30%, respectively. Per-process peaks stayed stable as concurrency
rose; the four-process sums are deliberately conservative and are not claims
about synchronized host RSS. The sub-millisecond `generated_book.pdf` timing
delta is especially noise-sensitive, while the repeated 24-page compressed
fixture provides the strongest CPU evidence.

A content-safe differential probe found identical non-whitespace character
sequences for all three PDFs. Exact output differed for the compressed fixture
and `sample.pdf` because `lopdf` joins positioned glyph arrays with different
whitespace than `pdf-extract`; the Romanian diacritics initially suspected as
missing were present as separately spaced glyphs. The node documentation now
states that positioned-PDF whitespace is best-effort rather than a stable byte
identity. No exact extracted content or local path is retained in benchmark
results.

Focused validation passed all three release-worker Rust tests and seven Bun
benchmark parser/selector/schema/checksum tests (18 expectations). The actual
PDF-only selector produced both 84-record matrices, all successful, and the
measurement-identical differential probe verified character preservation
without retaining content. The affected Lua example validates; the current
release worker rebuilds successfully; `cargo fmt --all -- --check`, the
446-module size policy, exact all-target Clippy, and `git diff --check` pass.
No storage service, deployment, or full integration gate was run because this
opt-in benchmark/tooling change is not an integration-boundary push.

### IF-093 — Non-ASCII CID glyph extraction depends only on local samples

**Status:** Resolved on 2026-08-04.

IF-092 verified that `lopdf` preserved the non-whitespace character sequence in
the ignored Romanian `data/samples/sample.pdf`, but this evidence cannot run in
CI or a clean checkout. The committed PDF fixtures use simple Latin fonts and
do not exercise an Identity-H CID font, a ToUnicode CMap, multi-byte source
codes, or glyph fragments split across positioned text-showing operations.

Required outcome:

- generate and commit a byte-deterministic, compact PDF with an Identity-H CID
  font, a ToUnicode CMap, Romanian diacritics, and deliberately fragmented text;
- pin its size and SHA-256 identity in the benchmark manifest;
- prove `extract_pdf` preserves the expected non-whitespace Unicode character
  sequence in both text and Markdown formats without freezing incidental
  whitespace placement;
- include it in the documented opt-in extraction corpus and keep examples and
  public resource contracts unchanged.

Resolution: the deterministic benchmark generator now emits
`cid-unicode.pdf`, a 1,793-byte, single-page PDF with an Identity-H Type0 font,
a CIDFontType2 descendant, a ToUnicode CMap, two-byte source codes, and the
Romanian text `Activă București Țară Știință` split across separate `TJ`
operations. Its SHA-256 identity
`2bfe6e8ba3e6792236c9b3c3fe69b36f301ceda8cf789515443019af8cfa1886`
and size are pinned in the benchmark manifest. Regeneration left all four
pre-existing fixture files byte-identical.

The PDF fixture builders now live in a focused 155-line submodule while the
general fixture coordinator remains 177 lines. A runtime regression executes
the public `extract_pdf` node in both text and Markdown modes, removes only
whitespace, and requires the complete expected Unicode sequence. It therefore
detects lost or corrupted multi-byte glyphs without freezing `lopdf`'s
best-effort whitespace placement. The benchmark guide lists positioned
CID/ToUnicode glyph fragments as part of the committed corpus; no Lua example
or public node parameter changed.

Focused validation passed three benchmark-worker Rust tests; three PDF node
tests including both Unicode output modes; seven Bun benchmark selector,
schema, and five-fixture checksum tests (20 expectations); byte-for-byte
fixture regeneration; and a three-record release-mode PDF benchmark smoke run
covering the baseline plus both committed PDFs under schema 2. The affected Lua
example validates. Poppler inspection identifies one unencrypted PDF 1.4 page
and one Identity-H CID TrueType font with a usable Unicode map. The release
worker build, `cargo fmt --all -- --check`, the
446-module size policy, exact all-target Clippy, and `git diff --check` pass.
No storage service, deployment, or full integration gate was run because this
fixture/tooling regression is not an integration-boundary push.
