# IronFlow Architecture

## Overview

IronFlow is a lightweight, high-performance workflow engine that combines a Rust execution core with Lua scripting for flow definitions. It follows the same proven pattern used by Neovim, OpenResty/Nginx, Redis, and game engines like Roblox — a fast, safe systems language for the runtime, and a minimal scripting language (~20 keywords) for the user-facing layer.

The engine provides DAG-based task scheduling with parallel execution, dependency management, retries with exponential backoff, conditional routing, and pluggable state persistence. Flows are defined in plain Lua scripts, loaded at runtime without recompilation, and executed in a sandboxed environment where scripts cannot access the filesystem or network unless explicitly granted through nodes.

IronFlow ships as a single binary and does not require a Python, Node.js, or container runtime. Most nodes need only that binary; `pdf_to_image` and `pdf_thumbnail` additionally require a native Pdfium library. It runs in CI/CD pipelines, edge servers, air-gapped environments, or as a long-running API service behind Docker, Railway, or Fly.io.

## Design Principles

1. **Rust core, Lua surface** — All nodes and the execution engine are implemented in Rust. Flows are defined in Lua scripts.
2. **Single binary** — Ship one executable without a language runtime; document native prerequisites for nodes that need them.
3. **Sandboxed execution** — Lua scripts run in a restricted environment. No filesystem or network access unless explicitly granted through nodes.
4. **Async-first** — The engine uses `tokio` for async execution. Synchronous
   CPU/blocking work is isolated from runtime workers and must cooperate with
   cancellation.
5. **Pluggable persistence** — State storage is trait-based. JSON file storage ships by default; Redis is optional.
6. **Bounded responsibilities** — Keep modules cohesive and small enough to
   review independently: target at most 300 physical lines and split distinct
   responsibilities rather than let a file exceed 400 lines. CI treats 301–400-line
   production modules as reviewed, exact-count exceptions and rejects anything
   larger. LOC is a review trigger; cohesion, cognitive complexity, and useful
   test extraction remain the actual design criteria.

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
│  │ JSON/SQLite  │ │Null Store│ │PG/Redis   │  │
│  └──────────────┘ └──────────┘ └───────────┘  │
└─────────────────────────────────────────────┘
```

## Key Components

### 1. Workflow Engine (`engine/`)

The core execution engine responsible for:
- Parsing Lua flow definitions into a DAG
- Topological sorting of normal and recovery edges with cycle detection
  (Kahn's algorithm)
- Structured parallel execution of independent tasks via `FuturesUnordered`
- Concurrency control via semaphores
- Context management (shared HashMap passed between nodes)
- Duplicate step name detection at parse time
- Planned on-error recovery (`on_error()` adds a dedicated handler to the DAG)
- Dependency-failure propagation (downstream steps are skipped)
- Supervised run finalization across workflow failure, infrastructure failure,
  panic, waiter detachment, and explicit cancellation

#### Run lifecycle and cancellation

`WorkflowEngine::start()` validates and initializes a durable `Pending` run,
then returns a `RunHandle` backed by a detached coordinator. `execute()` is the
compatibility wrapper that starts the run and waits for the handle.

- Dropping a `RunHandle` or a future waiting on it detaches the waiter; it does
  not abandon or cancel the workflow.
- `RunHandle::cancel()` explicitly stops structured in-flight work and persists
  the run and unfinished tasks as `Cancelled`.
- A run-level deadline uses the same cancellation outcome. Lease-renewal
  timeout, backend error, or ownership loss is instead an infrastructure stop:
  it cannot be downgraded by a simultaneous cancellation signal and converges
  to durable `Stalled` state through owner finalization or lease reconciliation.
- A configured step timeout creates one deadline shared by all of that step's
  attempts and retry backoffs. Structured child runs use cancel-on-drop waits,
  so a parent timeout does not silently detach them. Explicitly detached
  `subworkflow(wait = false)` work remains independent by contract.
- Controlled node failures produce `Failed`. Executor panics, task-state write
  failures, and other infrastructure faults produce `Stalled`.
- One finalizer repairs every initialized non-terminal task, persists the final
  context, retries the terminal status write, and only then emits the
  best-effort `RunFinished` event. The state store is the source of truth.
- Engine-owned initialization, task, context, and non-terminal status writes
  each have a 10-second persistence budget. Cancellation, a run deadline, or
  lease loss also preempts the complete execution future even when the awaited
  node/store future does not cooperate. Finalization has one 15-second budget,
  after which local ownership is released for durable reconciliation.

Every built-in durable store establishes a unique execution-owner lease before
the coordinator starts. The lease lasts 90 seconds and the coordinator
refreshes it every 30 seconds. Owner-checked status, task, and context mutations
prevent a late coordinator from writing after its lease expires. `serve`
allows up to 30 seconds for one reconciliation pass before accepting traffic,
then repeats a 30-second-bounded pass every 30 seconds. An expired
`Pending`/`Running` run becomes `Stalled`; unfinished running tasks become
`Failed` and other unfinished tasks become `Skipped`.

SQL uses the database clock and Redis uses `TIME` for lease creation, renewal,
fencing, and reaping, so application-host clock skew cannot transfer ownership
early. The JSON backend uses process UTC time and coordinates processes sharing
one local store directory with an owner file plus a cross-process file lock; an
external holder can delay that lock for at most five seconds. Use SQL or Redis,
not a shared JSON directory, across hosts.

Reconciliation makes state durable after an ungraceful process/host
termination, but state and `EventStore` are intentionally separate systems. A
reaper cannot atomically publish the corresponding terminal event, so run state
remains authoritative when event publication is missing or times out.

### 2. Node System (`nodes/`)

Each node is a Rust struct implementing the `Node` trait:

```rust
#[async_trait]
pub trait Node: Send + Sync {
    fn node_type(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput>;
}
```

Nodes are registered in a `NodeRegistry` and exposed to Lua as callable factory functions. 102 built-in nodes are provided across HTTP, shell, file, S3, S3 vector, MCP, data transform, iteration, caching, conditional, timing, code execution, markdown, XML, YAML, HTML sanitization, date/time, encoding, document extraction, image processing, database, AI, subworkflow/tool dispatch, notification, and utility categories. The `pdf_to_image` and `pdf_thumbnail` nodes require the native Pdfium library at runtime.

### 3. Lua Runtime (`lua/`)

- Uses `mlua` crate with Lua 5.4
- Flow definitions return a `Flow` object with steps and dependencies
- Opt-in context interpolation for documented node parameters, using explicit
  paths (`${ctx.user.name}`, zero-based `${ctx.items[0].name}`, and
  `${ctx["key.with.dots"]}`)
- Sandbox restricts access — `os`, `io`, `debug`, `loadfile`, `dofile` are removed
- `env(key)` function exposed for reading environment variables
- Function handlers — Lua functions passed directly as step handlers are compiled to bytecode and executed as `code` nodes
- `code`, `foreach`, and step-owned nested-flow parsing execute on Tokio's
  blocking pool rather than runtime workers. Their instruction hook observes
  both resource limits and the executor's step deadline/drop-cancellation
  signal.

### 4. State Store (`storage/`)

```rust
#[async_trait]
pub trait StateStore: Send + Sync {
    async fn healthcheck(&self) -> StorageResult<()>;
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()>;
    async fn init_run_owned(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
        lease: &RunLease,
    ) -> StorageResult<()>;
    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()>;
    async fn set_run_status_owned(
        &self,
        run_id: &str,
        status: RunStatus,
        owner: &str,
    ) -> StorageResult<bool>;
    async fn renew_run_lease(&self, run_id: &str, lease: &RunLease) -> StorageResult<bool>;
    async fn reconcile_expired_run_leases(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> StorageResult<usize>;
    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()>;
    async fn upsert_task_owned(
        &self,
        run_id: &str,
        task: &TaskState,
        owner: &str,
    ) -> StorageResult<bool>;
    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context>;
    async fn update_ctx(&self, run_id: &str, updates: &Context) -> StorageResult<()>;
    async fn update_ctx_owned(
        &self,
        run_id: &str,
        updates: &Context,
        owner: &str,
    ) -> StorageResult<bool>;
    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo>;
    async fn list_runs(&self, filter: Option<RunStatus>) -> StorageResult<Vec<RunInfo>>;
    async fn list_run_summaries(&self, filter: Option<RunStatus>) -> StorageResult<Vec<RunSummary>>;
    async fn list_run_summaries_page(
        &self,
        query: &RunListQuery,
    ) -> StorageResult<RunSummaryPage>;
    async fn delete_run(&self, run_id: &str) -> StorageResult<()>;
    async fn prune_before(&self, cutoff: chrono::DateTime<chrono::Utc>) -> StorageResult<usize>;
    async fn claim_schedule(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> StorageResult<bool>;
}
```

The owned methods are the execution path used by `WorkflowEngine`. Their
boolean result is a fencing decision: `false` means the caller no longer owns
the run and must stop. For source compatibility, the trait defaults delegate to
the corresponding unowned mutation, treat renewal as successful, and reconcile
nothing. A custom store using those defaults remains usable in one process but
does **not** gain crash recovery or multi-replica fencing; it must override the
owned methods atomically to claim those guarantees. The built-in JSON, SQL, and
Redis stores do so.

Networked built-in state and event stores also override `healthcheck` with a
real `SELECT 1` or Redis `PING`. `/health/ready` bounds both probes to two
seconds and combines them with the process admission state. Custom stores keep
the compatibility no-op probe unless they override it; an embedding that wants
dependency-sensitive readiness must provide that implementation.

The two vector-returning methods remain compatibility/maintenance primitives;
user-facing listing must use the required bounded page method. `RunListQuery`
always contains a non-zero page size and an optional filter-bound keyset
cursor. The final ordered set passed into `RunSummaryPage` retains at most
`limit + 1` summaries, using the extra record only to produce the next cursor;
this bound does not imply that every backend examines or temporarily merges
only `limit + 1` records. Ordering normalizes `started` to UTC
microseconds, sorts it descending with missing timestamps last, and then sorts
run ID descending. Timestamps that differ only below microsecond precision are
therefore tied and use the ID order. SQL applies filtering, cursor, ordering,
task counts, and `LIMIT` in the query. JSON binary-searches an immutable
fixed-record base, reads at most `limit + 1 + K` base entries, and merges its
checksummed delta, where `K <= 128` is the overlay entry count. A clean page
therefore uses O(log N + page size + K) catalog reads and O(page size + K)
memory without enumerating the store directory.

Both `StateStore` and `EventStore` return `StorageResult`. `StorageError`
preserves one of five boundary-safe categories: `InvalidInput`, `NotFound`,
`Backend`, `Corruption`, or `Conflict`. Implementations sanitize diagnostics
before returning them and never retain a credential-bearing driver source
chain. API consumers map `InvalidInput` to `400`, `NotFound` to `404`, and
`Conflict` to `409`; the SSE preflight refines a supplied event-cursor
`NotFound` to `410 Gone`. Backend and corruption failures become generic `500`
responses with an opaque error ID. The API logs the sanitized diagnostic once
with that ID and mirrors it in `X-Error-ID` for correlation.

Implementations:
- **JsonStateStore** — Local file-based storage with one main JSON record and
  one derived summary sidecar per run, per-file atomic replacement, and an
  in-process `RwLock`. Both files carry the same opaque revision and SHA-256
  digest of the public summary. Listing uses the sidecar only when a bounded
  primary header matches both values and the sidecar content recomputes to the
  digest. Missing, syntactically invalid, schema-unusable, or revision/digest-
  mismatched caches cause a full primary decode and best-effort repair; an
  explicit string sidecar ID that disagrees with its filename is corruption
  and does not fall back. Bounded pages use a checksummed, fixed-record catalog
  base with one global and six status-specific ordered sections plus a
  checksummed, coalesced delta capped at 128 distinct run IDs. The page path
  probes its cursor in logarithmic time, range-reads at most
  `limit + 1 + K` base records, merges the K-entry overlay, and verifies the
  selected current summaries. A dirty, missing, stale, or malformed base,
  delta, or version-2 state token is rebuilt from authoritative primary records
  under the shared catalog file lock.
- **NullStateStore** — In-memory, transient (used by subworkflow nodes)
- **SqlStateStore** — SQLite/Postgres-backed store with run rows containing
  context and separate task rows, avoiding full run-record rewrites on task
  updates. A numeric
  microsecond sort key and the `{prefix}runs_started_idx` and
  `{prefix}runs_status_started_id_idx` indexes support database-side keyset
  pages. Legacy rows are backfilled in bounded batches during schema setup and
  again before a page query, so rows inserted later by a mixed-version writer
  cannot move between the timestamp and `NULL` partitions while paging. A
  shared per-run mutation lock serializes task upserts with deletion and prune
  (`FOR UPDATE` on PostgreSQL and a writer-locking no-op update on SQLite), so a
  task cannot be inserted after its parent run disappears. A single run
  deletion removes its tasks and run row in one transaction; `prune_before`
  locks eligible runs and deletes every selected run/task set in one
  transaction, so any failure rolls the whole prune back.
- **RedisStateStore** — Redis-backed (optional, `redis` feature flag). Uses a
  Redis Hash per run plus native global and per-status Sorted Set indexes.
  Sorted Set members encode the normalized microsecond timestamp and run ID,
  so `ZREVRANGEBYLEX` reads only the ordered `limit + 1` page in the normal
  path. Derived keys live under `{prefix}run_catalog:v1:`, outside the
  historical `{prefix}runs:{run_id}` namespace. The legacy Set catalog remains
  the source for a one-time lazy rebuild; constant-cost cardinality checks also
  rebuild missing derived indexes before a page is served. The full record,
  compact summary, and ordered-index entry are
  committed together through immutable-incarnation, revision-token CAS, so
  concurrent task/context writers rebase with capped exponential jitter
  instead of losing updates and stale work cannot cross delete/recreate.
  Initialization, status changes, deletion, and expired-index cleanup update
  the global/status indexes atomically; a cleanup that loses to
  reinitialization rereads the live incarnation. Existing hashes without
  tokens remain readable and acquire
  deterministic legacy-incarnation plus revision fields on their first
  successful mutation. UUID-like historical key segments remain unchanged,
  while unsafe/reserved run IDs use an injective encoded namespace. Valid
  non-aliasing raw legacy keys migrate atomically after their embedded run ID
  is checked. An occupied encoded target must carry the requested embedded ID;
  otherwise reads, initialization, and deletion fail before mutation.
  Accessing the historical owner first migrates it and frees the collision.
  Supports a configurable key prefix, bounded positive sliding TTL, and
  auto-reconnecting connection pool. While a run is owned, its hash TTL is the
  larger of configured retention and the remaining lease plus a 90-second
  reaper safety margin; with no configured retention it stays persistent.
  Terminalization releases the lease and restores ordinary retention.
  Production state deployments should use
  Redis `maxmemory-policy noeviction`; independent eviction of primary catalog
  keys is outside the storage durability contract. Expired hash entries are
  removed from the persistent catalog when paging encounters them. In
  addition, every steady-state page request advances a persistent maintenance
  cursor by up to 32 catalog members independently of the user-visible cursor.
  Each maintenance cycle records a high-water member, so continuous newer
  inserts cannot prevent wraparound. Every claimed valid live member receives
  a revision-safe full catalog/status-index repair, while expired members are
  removed. A revision conflict is deferred to a later cycle rather than
  retried without a bound; the winning state CAS has already published its
  catalog representation atomically. Repeated requests therefore repair
  balanced index drift and cold TTL leftovers with bounded incremental work. A
  missing or inconsistent derived catalog starts the documented one-time
  legacy Set scan under a renewable, owner-checked Redis lease; readers accept
  only its finalized
  generation and recheck that generation before returning a page.

#### Run-list major-version boundary

The cursor-page contract is intentionally incompatible with the earlier
offset contract and must be released on a major-version boundary. The HTTP
endpoint replaces `offset` with filter-bound `after`, removes exact `total`,
and returns `has_more` plus `next_cursor`. CLI JSON changes from a top-level
array of full run records to a page envelope of summaries. There is no
unbounded compatibility mode; API and CLI pages are constrained by the
positive `IRONFLOW_MAX_LIST_RECORDS` limit, which defaults to 100.

This is also a Rust API break. External `StateStore` implementations must add
`list_run_summaries_page(&RunListQuery)`, and embedded API construction must
provide a validated `ListingPolicy` through both `AppState` and
`ServeOptions`. Downstream HTTP clients, CLI parsers, trait implementations,
and struct-literal constructors must migrate together before adopting this
major version.

#### JSON store filesystem boundary

Public API paths and `JsonStateStore` share one canonical run-ID validator.
The token is 1 through 128 bytes, contains only ASCII letters, digits, `-`, and
`_`, and must start and end with a letter or digit. Validation is byte-exact:
there is no trimming, Unicode normalization, or case folding. Engine-generated
IDs are UUIDv4, which are one valid subset. Invalid public path IDs become
`400 bad_request` after percent decoding and before any store call; valid IDs
that are absent remain `404 not_found`.

The JSON root must be a real directory rather than a symbolic link. Run record
and summary entries are also rejected when they are symlinks or otherwise not
regular files. Together with the run-ID grammar, this prevents a record name
from escaping the configured root; it does not claim protection against a
separate hostile process replacing filesystem entries during an operation.

Each main record and summary sidecar is serialized and synced in a temporary
file in the same directory. Initial main-record publication uses a hard link
that does not overwrite an existing main record; replacements and summary
commits rename the temporary file over their preflighted target. On Unix, the
containing directory is synced around publication as well. This provides
per-file atomic visibility under the underlying filesystem's hard-link/rename
semantics. The pair is not one filesystem transaction. Instead, the primary
record is authoritative and both payloads carry the same generated revision
plus a SHA-256 digest of the serialized public summary. Listing reads a bounded
primary prefix and uses the sidecar only when its revision and digest match the
header and recomputing the sidecar digest succeeds. A missing, syntactically
invalid, schema-unusable, or revision/digest-mismatched sidecar causes a full
primary decode and best-effort cache repair. A sidecar with an explicit string
run ID that disagrees with its filename is reported as corruption instead of
falling back. Full primary decoding validates
identity, revision, and digest.

The bounded listing fast path deliberately validates the committed header and
summary projection, not arbitrary bytes in the remainder of a primary that it
does not need to read. Suffix-only corruption can therefore remain invisible
to summary listing until `get_run` or a mutation performs a full decode; header
corruption and identity/digest mismatches on a full decode are surfaced. If a
summary commit or repair fails after the primary commit, the state mutation
remains successful, the failure is logged, and later listings derive the
summary from the primary when the fast-path proof is unavailable. Legacy
unversioned and revision-only records use the authoritative full-record path;
revision-only records are not repeatedly "repaired" into an unusable sidecar,
and their next mutation upgrades both files.

Bounded pages are driven by `.ironflow-run-catalog-v1.bin`, an immutable derived
projection of canonical run ID, status, and microsecond-normalized start time.
Its checksummed header identifies a base generation and offsets one global plus
six status-specific sections; each fixed-size record has its own checksum.
`.ironflow-run-catalog-v1.delta` stores at most 128 coalesced upserts or
tombstones, sorted by canonical run ID, with checksummed header/entries and a
revision bound to the base generation. The matching version-2
`.ironflow-run-catalog-v1.state` clean token binds both IDs and fingerprints the
directory, base, and delta. Page reads verify the token before and after
binary-search/range-read selection, retrying when a concurrent writer changes
it. They read at most `limit + 1 + K` base entries and merge the K-entry delta,
so a clean page is O(log N + page size + K), uses O(page size + K) memory, and
does not enumerate the store directory (`K <= 128`).

Participating `JsonStateStore` writers hold
`.ironflow-run-catalog-v1.lock`, mark the projection dirty before changing an
authoritative record, then publish the derived change or confirm that its
projection is unchanged. Initialization, status changes, and deletion normally
read and atomically replace only the O(K) delta. Repeated changes to one run ID
coalesce into one entry; the 129th distinct overlay ID performs an O(N)
compaction into a new immutable base and empty delta. Task/context-only changes
leave both base and delta bytes unchanged and refresh the clean fingerprint.
This local file lock coordinates store instances on one filesystem, but it is
not a distributed lock and does not protect against a hostile external process.
The JSON backend is therefore best suited to local, moderate-cardinality
workloads; use SQL or Redis for sustained high-write or high-cardinality use.

Missing, dirty, stale, or malformed base/delta metadata is rebuilt lazily from
authoritative primary records. The state-format transition is intentionally
not a mixed-writer protocol: stop all writers before upgrading to or
downgrading from version 2, then let the first current process rebuild. For
explicit repair, stop all writers, make a backup, and call
`JsonStateStore::rebuild_run_summary_catalog()`; the rebuild publishes a fresh
base and empty delta. If a base, delta, state, or lock path has been replaced by
a symlink/non-regular entry, remove it while offline first: IronFlow rejects
unsafe metadata instead of following it. Ultimate durability remains subject
to the filesystem and storage hardware.

On Unix, the store directory is mode `0700`, and committed main, summary,
catalog base, delta, state, and lock files are mode `0600`. These numeric
ownership-mode guarantees are Unix-only.
On non-Unix platforms, operators must configure equivalent filesystem ACLs;
all platforms remain subject to their filesystem's rename guarantees.

Historical engine-generated UUID records need no migration. During listing, a
pre-existing JSON filename with a noncanonical ID is a corruption error rather
than an ignored entry. Direct-store custom IDs outside the grammar require an
offline, backed-up export/reimport or a coordinated rename of both main and
summary filenames plus their embedded IDs.

State backend selection is controlled by the `store_backend` config field or `IRONFLOW_STORE` environment variable. SQLite/Postgres table names use `IRONFLOW_SQL_TABLE_PREFIX` / `sql_table_prefix`, defaulting to `ironflow_` so existing names such as `ironflow_runs` are preserved.

Run execution events use a separate `EventStore` abstraction so monitoring can
be scaled independently from run persistence. Besides publish and replay, the
trait requires idempotent, counted `delete_run`: it removes every retained
payload for a run, installs a late-publication fence, and returns the number of
payloads removed. Callers must give every logical event a non-empty ID that is
never assigned to another event in the same run, even after retention or TTL
expiry; engine-generated UUIDv4 event IDs satisfy this obligation. An exact
publication retry is idempotent while the identity remains retained. Bounded
and TTL-backed stores can detect conflicting ID reuse only while they retain
the prior identity. Replay with an unknown, expired, or cross-run cursor
returns `StorageErrorKind::NotFound`.

- **MemoryEventStore** — In-memory events for local or single-instance
  deployments. A global queue retains at most `event_memory_capacity` entries
  across events and deletion fences (default 10,000,
  `IRONFLOW_EVENT_MEMORY_CAPACITY`) and also enforces a fixed 64 MiB retained
  heap estimate including string allocations and deque capacity. Oldest-first
  eviction applies when either bound is exceeded; an individually oversized
  event is rejected, and a cursor or fence that has been evicted is no longer
  available.
- **SqlEventStore** — SQLite/Postgres-backed event replay for shared API
  deployments. Each publication receives a transactionally allocated,
  per-run monotonic sequence; replay resolves the opaque event ID to that
  sequence and event identity is scoped by `(run_id, id)`, so two runs may use
  the same opaque ID. Migration adds the nullable sequence column and repairs
  legacy rows in 256-row transaction batches, ordered by the former
  `(timestamp, id)` contract. A partial `(run_id, timestamp, id)` index bounds
  the remaining-null probe, and repair also runs before publish/read. A unique
  `(run_id, sequence)` index enforces replay order. Startup verifies that the
  managed index name resolves to that exact live unique index; publication
  rejects negative or trigger-altered counters before committing, and legacy
  repair rejects a guarded update/delete that makes no progress.

  Replacing a legacy global `PRIMARY KEY(id)` is a guarded, locked schema
  migration. SQLite rebuilds the table only after rejecting unknown columns,
  uniqueness/index extensions, triggers, and foreign-key relationships it
  cannot preserve. PostgreSQL changes the key in place after rejecting extra
  uniqueness, exclusion constraints, and a deferrable primary key; dependency
  failures roll the transaction back. This identity change requires a
  coordinated writer stop:
  pre-migration binaries still issue `ON CONFLICT(id)` and are not compatible
  after the key becomes `(run_id, id)`. Once duplicate event IDs exist across
  runs, an old binary/schema cannot be restored without offline data
  transformation; do not treat the retained legacy read index as downgrade
  support.
- **RedisEventStore** — Redis-backed event replay (optional, `redis` feature
  flag) using a per-run list, event ID index, sequence, and owner-bound layout
  marker. One preflighted Lua script atomically appends the payload, cursor
  mapping, sequence, ownership, and TTLs. Retrying an identical nonempty event
  ID is idempotent; conflicting payloads and deterministic
  key/type/sequence failures are rejected before mutation. Legacy migration
  requires Redis 6.2 or newer because it uses `LMOVE`.

  Migration state and quarantine use deterministic keys derived from the exact
  run ID. Before any payload is read, one Lua operation records the generation,
  cursor, policy, absolute TTL deadline, and source-key presence, then renames
  every present family component into exact-run quarantine. A deterministic
  snapshot without its state is treated as orphaned corruption, never as an
  empty stream; IronFlow leaves it intact for manual recovery.

  Automatic handling is limited to alias-safe physical families and unsafe-ID
  families with an exact owner marker. An exactly owner-marked encoded family
  is accepted as current, and an exactly owned optional raw family can migrate
  into the encoded namespace. An ownerless unsafe encoded family is ambiguous
  and fails with manual-migration guidance. An optional raw candidate without
  the exact requested owner remains untouched and is ignored, leaving the
  injective encoded namespace available.

  Each validation fetch uses bounded head `LINDEX` reads: at most 128 events
  and at most 1 MiB of serialized payload are returned to Rust for full
  deserialization. Redis has no per-element length metadata, so it must read
  one oversized list element once to discover that it exceeds 1 MiB; the
  element is not returned as a valid batch. Before rotating a validated batch,
  the script persists a new generation plus a pending count, digest, cursor,
  and rolling-digest intent. It then rotates that batch from list head to tail
  with same-list `LMOVE`. The next transition verifies the tail against the
  pending intent before committing the cursor. Disconnects and command faults
  therefore resume or fail closed rather than guessing how far the list moved.

  The complete list receives two rolling payload/index-digest passes. Processing
  every item once per pass restores the original list order; digest mismatch
  restarts bounded validation and repeated drift eventually blocks migration.
  The acknowledgement of the last verification batch either restarts or
  finalizes in that same Lua invocation, so there is no exposed
  verified-but-not-finalized snapshot interval.
  A schema, owner, cursor, oversized-payload, or Rust-deserialization failure
  enters bounded reverse restoration: confirmed batches rotate from tail back
  to head in windows bounded by both 128 elements and 1 MiB, with their own
  generation and pending intent, before the family is renamed to its original
  namespace and the corruption is reported. Legacy fetch replies are decoded
  as bytes, so invalid UTF-8 reaches Rust validation and restoration instead of
  stranding quarantine. One public operation confirms at most 32 bounded steps
  before returning `Conflict` with explicit retry guidance.

  The shortest source TTL is captured as an absolute Redis-time deadline before
  quarantine and every moved component is immediately aligned to that deadline.
  Renames and rotations do not refresh it; restoration and finalization may only
  shorten components to the deadline or their shortest remaining TTL. If a
  complete snapshot was persisted or extended out of band past the deadline,
  IronFlow expires it instead of restoring or adopting it. Once the governed
  source and snapshot namespaces are absent after the deadline, the next access
  removes the persistent progress record and treats the retained stream as
  expired. Partial families still fail closed. Command-capability probes
  operate only on absent keys and never create probe data.

  Migration requires a coordinated stop of pre-protocol event writers. If one
  recreates the source or destination while quarantine exists, migration is
  blocked and every family is preserved rather than merged, overwritten, or
  deleted. Pending checks and two digest passes reject changes they observe,
  but deterministic quarantine keys are internal protocol state and must not be
  edited directly. Operators must stop the old writer and resolve any recreated
  or partially modified family before retrying.

  Cursor lookup and current-layout list reads remain one script-backed
  snapshot. Current-layout deletion installs the fence and removes the
  list/index/sequence/layout namespace in one preflighted Lua operation;
  large-key freeing uses `UNLINK` only after command capability is proven
  before any mutation.

Event backend selection is controlled by the `event_store` config field or `IRONFLOW_EVENT_STORE` environment variable. SQL event URLs use `event_store_url` or `IRONFLOW_EVENT_STORE_URL`, and SQL event tables use the same SQL table prefix as the state store; Redis uses the existing `REDIS_URL`, `REDIS_PREFIX`, and `REDIS_TTL` settings.

#### Run and event deletion boundary

`DELETE /runs/{id}` uses the shared lifecycle coordinator rather than calling
only the state store. It deletes state first and then calls the event store's
idempotent deletion. If event cleanup fails after state committed, the request
fails but a retry still calls event deletion even though state is now absent;
the retry succeeds when it removes an orphaned event stream. An absent state
record with no orphaned events remains `404 not_found`. A successful event
delete fences a workflow event racing behind it, so a late publication cannot
undo deletion while that backend retains the fence.

JSON, SQL, and Redis inspect status and lease under the same backend-atomic
deletion fence. A non-terminal run with an unexpired execution-owner lease is
not removed and the API returns `409 conflict`; its state, lease, and events
remain intact. Terminal runs and non-terminal runs whose lease has expired are
deletable. This lets deletion safely race both heartbeat renewal and expired-
lease reconciliation without stripping ownership from a live coordinator.

Fence retention follows the selected event backend's durability boundary. SQL
fences are durable rows retained until operator cleanup. Redis fences are
persistent when `REDIS_TTL` is unset and otherwise expire with that TTL. Memory
fences share the bounded in-process queue, so they disappear on restart or
oldest-first eviction. Persistent SQL events otherwise require explicit
deletion; Redis can additionally expire event keys through `REDIS_TTL`.
Embedded callers that need the HTTP lifecycle semantics should use
`storage::lifecycle::delete_run`; direct `StateStore::delete_run` and
`prune_before` remain state-only maintenance operations.

#### SSE delivery contract

`RunEvent.id` is an opaque resume token. The SSE adapter preserves the selected
backend's current stable read order: it keeps a bounded queue for the complete
fetched page, drains that queue in order, and only then asks the store for
another page using the last emitted ID as an exclusive cursor.
Memory and Redis use append position, while SQL uses a durable monotonic
per-run publication sequence. Event timestamps remain payload metadata rather
than replay-order keys. Concurrent task execution can still publish in a
different order between runs; the sequence records the order chosen by the
backend rather than redefining DAG/context precedence.

At the HTTP boundary, a non-empty `Last-Event-ID` header overrides the
bootstrap `?after=` query cursor; an empty header is treated as absent. The
handler performs and retains its initial event-store page before committing the
SSE response. That makes run absence, cursor expiry, backend unavailability,
and corrupt storage normal typed HTTP errors rather than a misleading `200`.
Unknown, wrong-run, and expired cursors intentionally share `410 Gone` /
`event_cursor_gone`, because an event backend generally cannot distinguish
those cases after retention, restart, or TTL expiry.

After the response has started, storage and encoding faults cross a different
boundary: the adapter emits one safe, ID-less `stream_error` control event and
closes. Backend, corruption, conflict, and serialization diagnostics remain
server-side under the event's opaque error ID; the expected
`event_cursor_gone` condition has no error ID. Only a backend failure is marked
retryable; corruption, conflict, cursor loss, and serialization failure are
permanent for that connection and are marked non-retryable. An ID-less control
event cannot advance the domain replay cursor. A serialized `run_finished`
event is the normal EOF marker; after it is flushed the stream closes. If state
is already terminal and no terminal event is retained, the adapter drains
available events and closes without inventing one.

When its queue is empty for a non-terminal run, the adapter waits one second
between store reads. After 15 seconds without an event frame, the transport
writes an ID-less SSE keep-alive comment. Neither polling nor keep-alive
comments change cursor state, and both stop when the stream is closed or
disconnected. The exact client-facing examples and error frame are specified
in `docs/CLI_REFERENCE.md`.

Redis atomicity is scoped to one state mutation or one event operation. Run-state persistence and event publication remain separate trait calls and are not one cross-backend transaction. Configured TTLs must be between 1 and 99,999,999,999 seconds so all Lua/Redis expiry conversions are preflighted and exact. The multi-key scripts target standalone Redis; Redis Cluster is not currently supported because the existing run/index and event keys are not guaranteed to share a hash slot. Host-crash durability remains determined by the Redis persistence configuration.
Rolling deployments must not mix pre-CAS and CAS-capable Redis writers: an old binary does not advance revision tokens. Reads and a coordinated upgrade from legacy records are supported; mixed-version writes are not.

### 5. Scheduler (`scheduler/`)

`ironflow serve` evaluates configuration-only `schedules:` entries every 30
seconds. Each scheduler keeps a local wall-clock watermark per schedule,
enumerates only occurrences still reachable within that schedule's grace
window, and caps one tick at 64 occurrences; excess occurrences are skipped,
not queued. Cron expressions have five fields and traditional crontab
day-of-month/day-of-week OR semantics; time zones use IANA names. A repeated
fall-back wall-clock instant resolves to the earlier occurrence, while a
spring-forward gap resolves to the first real instant after the gap.

Configuration admits at most 256 schedules. Names are at most 48 UTF-8 bytes,
flow paths 1,024 bytes, cron expressions 256 bytes, timezone names 128 bytes,
and serialized initial contexts 65,536 bytes. Grace is bounded to
60..=604,800 seconds and converted to signed duration only after validation.
The name limit also keeps the existing rolling-upgrade-compatible JSON claim
filename below common filesystem component limits.

For every due instant, the scheduler first calls
`StateStore::claim_schedule(name, key, ttl_seconds)`. The key contains the time
zone and resolved local minute, so replicas sharing a store compete for the
same identity. JSON uses exclusive file creation, SQL uses a unique row, and
Redis uses `SET NX EX`; the transient null store always succeeds and is suitable
only when no peer scheduler exists. Claim retention exceeds the grace window so
a late replica cannot re-fire a recently reaped instant.

JSON keeps that rolling-upgrade-compatible exclusive file as the authoritative
lock and writes separate SHA-256 schedule shards with hourly cleanup buckets.
Pre-index flat claims are indexed across all schedule shards in batches of 256;
a durable global completion marker removes that transitional scan once
migration reaches the end of the legacy namespace. The authoritative claim file
is never moved or renamed during migration.

SQL has a covering `(name, claimed_micros, claim_key)` retention index. For
both backends, each process sweeps a schedule at most once per
`clamp(ttl / 4, 60 seconds, 1 hour)` and deletes no more than 256 expired claims
per sweep; a zero TTL used by storage contract tests requests immediate
cleanup. Cleanup is best-effort and schedule-scoped, so a retention failure can
retain storage but cannot invalidate a committed claim or apply one schedule's
grace-derived TTL to another. Redis needs no sweep because expiry belongs to
each atomic claim key.

The claim is an admission/audit fence, while a deterministic run ID derived
from `(schedule name, resolved instant)` is the execution fence. Replicas that
observe an existing claim still converge on that run identity; the built-in
stores atomically allow only one initializer. A process dying after its claim
but before initialization therefore does not permanently consume the
occurrence. Existing or later-stalled executions are never replayed. Lateness,
overlap, process capacity, and flow-load failures may still skip an occurrence
instead of creating a backlog. The overlap check pages
through at most 256 non-terminal runs and tolerates read failures, so it is a
bounded best-effort guard rather than a distributed per-schedule lock. A claim
backend error is different: the watermark stops before that instant and retries
it on later ticks until it succeeds or falls outside grace.

Scheduled flows use the ordinary registry, engine, state store, event store,
run-admission semaphore, and flow-path confinement. Their initial context adds
`_schedule`, `_schedule_instant` for provenance and `_flow_dir` for relative
resources. A scheduled
run is detached from the tick loop after it starts, so a long-running flow does
not block later scheduler evaluations.

Schedule names evaluate concurrently, each under a 15-second wall-time budget.
A timed-out evaluation is indeterminate: its storage claim or run start may
have committed after the caller lost the result. IronFlow therefore logs the
timeout, reports each due instant as `TimedOut`, and advances that schedule's
watermark through the evaluated window instead of retrying and risking a
duplicate. Other schedule names continue independently. The tick task and API
server are supervised as one availability unit; an unexpected scheduler return
or panic ends `serve` with an error, while normal server completion requests and
awaits scheduler shutdown.

### Service lifecycle and replicas

HTTP handlers and the scheduler register each accepted top-level run with one
process `ServiceLifecycle`. Registration retains only cloneable cancellation
authority; the ordinary `RunHandle` waiter still owns completion and admission.
SIGTERM/SIGINT atomically closes readiness and execution admission. Axum stops
accepting connections, the scheduler exits its tick loop, and accepted runs may
finish for `IRONFLOW_SHUTDOWN_GRACE_SECONDS`. Remaining coordinators receive
the same cooperative cancellation signal as explicit run cancellation. Final
settlement and connection close are separately bounded so a defective task
cannot hold an orchestrator termination forever.

`IRONFLOW_REPLICA_MODE=true` is an operator assertion checked before stores are
opened. It accepts only PostgreSQL/Redis state plus PostgreSQL/Redis events;
JSON, SQLite, and memory are process/local-host backends. The detailed failure
and platform boundary is in [Replica deployment](REPLICA_DEPLOYMENT.md).

### 6. CLI (`cli/`)

Built with `clap`. Commands:
- `ironflow run <flow.lua>` — Execute a flow with `--context`, `--verbose`, `--store-dir`
- `ironflow validate <flow.lua>` — Check flow for errors (node types, dependencies, cycles)
- `ironflow list` — List bounded summary pages with `--status`, `--limit`,
  `--after`, and `--format` (table/json); no unbounded mode is exposed
- `ironflow inspect <run_id>` — Show run details as JSON
- `ironflow nodes` — List available node types
- `ironflow serve` — Start REST API server with `--host`, `--port`, `--flows-dir`, `--max-body`

Global flags:
- `--dotenv <path>` — Load environment variables from a specific file
- `-C, --config <path>` — Load configuration from a specific YAML file

CLI startup deliberately separates bootstrap discovery from full argument
resolution:

1. A bootstrap Clap pass validates the invocation and identifies an explicit
   `--dotenv` path. Without one, only `.env` in the current working directory
   is considered.
2. The selected dotenv file is parsed completely into memory. Existing process
   variables are preserved, and only missing keys are installed. An absent
   default file is allowed; a selected file that is unreadable or malformed
   fails startup without partially applying it.
3. Tracing is initialized from the merged environment, including `RUST_LOG`.
4. Clap repeats parsing against the merged environment and records whether
   each value came from an explicit argument, the environment, or a built-in
   default.
5. `ironflow.yaml` is loaded from `-C/--config` or the current working directory,
   and each setting is resolved as explicit CLI > process environment > dotenv
   > YAML > built-in default.

An explicit CLI value remains authoritative even when it equals the built-in
default. Config-only values, such as webhook definitions, skip the unsupported
sources in that order. This bootstrap occurs before any command opens a state
or event store.

### 7. REST API (`api/`)

Built with `axum`. Endpoints:
- `POST /flows/run` — Submit a flow for execution (via `source`, `source_base64`, or `file`); a bounded `Idempotency-Key` persists a request fingerprint and deterministic run identity
- `POST /flows/validate` — Validate a flow without executing
- `GET /runs` — List newest-first run summaries with optional `status`, `limit`
  (default 50), and filter-bound `after` cursor; the hard page cap comes from
  `IRONFLOW_MAX_LIST_RECORDS` (default 100)
- `GET /runs/:id` — Get full run details (context, tasks, timing)
- `GET /runs/:id/events` — Stream compact run/task lifecycle events over SSE
- `DELETE /runs/:id` — Delete run state and retained events, with `409` for a
  live non-terminal owner, retryable orphan cleanup, and a late-publication
  fence
- `GET /nodes` — List available nodes with descriptions
- `POST /webhooks/{name}` — Execute a webhook-mapped flow (configured in `ironflow.yaml`)
- `GET /health` — Backwards-compatible liveness alias
- `GET /health/live` — Process liveness without storage I/O
- `GET /health/ready` — Execution admission plus bounded state/event-store probes

Features:
- Exactly one source field required per request (mutual exclusion enforced)
- Base64 flow source support (`source_base64`) for escaping-free submission
- Configurable request body size limit (default 1 MB, `--max-body` flag)
- API key authentication for non-loopback servers via `IRONFLOW_API_KEY`
- Configurable CORS support via `IRONFLOW_CORS_ORIGINS` / `cors_origins`
- Request tracing via `tower-http`
- Typed storage-error mapping with generic, correlated internal-error responses
- Lua instruction, wall-clock, memory, and GC controls via `IRONFLOW_LUA_*` limits

## Data Flow

```
1. User writes flow.lua
2. CLI/API loads flow.lua into Lua VM
3. Lua script calls step()/depends_on() → builds FlowDefinition
4. Engine validates: duplicate names, unknown nodes, dependencies, dedicated
   recovery targets, and cycles in the combined normal/recovery graph
5. Topological sort of normal and recovery edges → execution phases
6. For each phase, preserve flow declaration order and capture one immutable
   phase-start context snapshot
7. Run ready tasks concurrently:
   a. Check route conditions and dependency failures against phase-start state
   b. Resolve node from registry
   c. Create one optional total deadline and pass config + the phase snapshot
      to `node.execute()`
   d. Persist terminal task state and buffer publishable output
   e. On failure: keep structured output private while retrying; after the
      terminal attempt, buffer its redacted output and retain the failure for
      its already-planned `on_error` handler or final run status
   f. Run a triggered recovery handler only after its declared dependencies;
      on success, resolve the source failure before downstream work proceeds
8. After every scheduled member settles, commit buffered output in declaration
   order; a later-declared same-phase writer wins a same-key collision
9. Final status written to state store
10. Context returned to caller
```

## Context Model

Context is a `HashMap<String, serde_json::Value>` that flows through the entire workflow:

- Every member and retry in a topological phase receives the same full,
  read-only phase-start context snapshot. Semaphore acquisition and completion
  timing cannot expose a peer's output.
- Successful node output and a typed `NodeFailure` terminal output are reduced
  into a phase-local per-key winner accumulator until the barrier. Intermediate
  retry output is discarded.
- Phase output is committed in flow declaration order. A later-declared
  same-phase writer deterministically wins a same-key collision; later phases
  may intentionally overwrite earlier values. Initial-context overwrites are
  normal writes, not parallel collisions.
- Per-task state remains task-local, but `IRONFLOW_MAX_TASK_OUTPUT_BYTES` can
  replace an oversized output record with a truncation marker. Cancellation or
  infrastructure failure before a phase barrier discards that phase's buffered
  shared-context publication while retaining already-persisted bounded task
  history.
- Output-size admission uses an aborting counting serializer. A truncation
  marker reports `_minimum_bytes = limit + 1`, not an exact original size,
  because counting stops as soon as the configured limit is crossed.
- Recovery handlers receive an invocation-local `_error_output` copy for typed
  failures in addition to the normal final context keys.
- Keys prefixed with `_` are reserved for engine internals (routes, conditions)
- Webhook requests persist `_webhook`, while explicitly allowlisted request
  headers enter an execution-only `_headers` overlay
- Optional webhook HMAC-SHA256 policies authenticate the exact bounded request
  body before JSON parsing, flow loading, run admission, or persistence; their
  environment-backed secret and signature header never enter context
- Execution overlays are merged into each node snapshot, propagated to child
  workflow engines, and removed before initial/final context persistence
- Literal overlay keys and values are redacted before task output/error, event,
  context, and public run boundaries; redacted output is what downstream steps
  receive, preventing simple credential laundering between tasks
- Documented Lua config fields can access context through `${ctx...}` path
  interpolation; configs are not rewritten globally
- The grammar supports dotted keys, zero-based array indexes, and JSON
  double-quoted bracket keys; expressions and fallback operators are rejected

### Binary artifacts

Large binary payloads are represented in context by an `ArtifactRef` rather
than an inline Base64 string:

```json
{
  "artifact_uri": "artifact://sha256/<64 lowercase hex characters>",
  "sha256": "<the same digest>",
  "size_bytes": 12345,
  "mime_type": "image/png"
}
```

The artifact-store facade streams sources to private local staging files (mode
`0600` on Unix and the configured directory's inherited ACL elsewhere) while
enforcing byte limits, hashing, and checking cancellation. Local mode publishes
them atomically as immutable `IRONFLOW_ARTIFACT_DIR/sha256/<digest>` files and
deduplicates equal content. S3 mode publishes the same local result, then
streams its verified handle to the configured bucket and
`<prefix>/sha256/<digest>` key. A remote write is successful only after a HEAD
probe verifies the digest/size metadata; an interrupted upload returns an error
without exposing a descriptor, while the local orphan remains eligible for the
offline retention sweep. Extraction, image/PDF, and transcription nodes
preserve either the full descriptor or its canonical URI until their tracked
blocking worker opens the artifact. The worker refuses links and non-regular
files, hashes the opened handle, compares that digest (and descriptor size when
present), rewinds it, and passes that same handle to the parser or decoder. The
store intentionally has no automatic eviction because a completed task or
recovered run can still reference an artifact. `ironflow artifacts prune`
performs explicit offline cleanup: it admits at most 100 old candidates, walks
retained runs in cursor pages, loads one full run at a time, and deletes only
digests absent from every context and task input/output. The required
`--confirm-offline` assertion is the race boundary; every process able to add a
reference must remain stopped for the scan/delete window. Publication is per
artifact rather than transactional across a node, so a later parser/page
failure can leave an unreferenced immutable object for that sweep.

Only the descriptor crosses context, task-history, Redis, or PostgreSQL
boundaries. In local mode, multi-host deployments must provide a durable shared
mount. In S3 mode, the object store is authoritative and each replica uses
`IRONFLOW_ARTIFACT_DIR` as private staging/cache. A cache miss streams the remote
body into a private temporary file, enforces descriptor/response/configured byte
ceilings, verifies SHA-256, and publishes locally before reopening the same
verified handle. Bucket, endpoint, prefix, and credentials remain deployment
configuration rather than descriptor fields, so every replica must select the
same namespace.
Artifact references bound workflow-context and persistence amplification; they
do not imply zero-copy processing. Consumers can still allocate decoded pixel
buffers, parser state, or semantic output subject to their node-specific
limits.

The local backend still treats its configured directory as a trusted process
boundary. Verified reads prevent same-size path replacement and pathname
time-of-check/time-of-use substitution from silently changing the input, but a
hostile process with the same OS identity can mutate an already-open inode
during or after verification. Operators must prevent workflows and unrelated
same-identity processes from mutating that directory. A flow execution identity
that can run arbitrary shell/file operations is not isolated from a store owned
by the same OS identity. Hostile multi-tenant deployments should use separate
execution identities and storage ACLs, or a separately authenticated artifact
service that returns authenticated content streams rather than shared paths.
S3 bucket policy or workload credentials authenticate the remote namespace;
verified content identity remains independent of that authorization. A shared
network filesystem retains the local process-trust boundary and is not
equivalent to a separately authorized object store.

The S3 lifecycle contract is deliberately narrow:

- IronFlow uses the standard AWS credential chain and an optional
  artifact-specific region/endpoint. Operators should scope the resulting
  bucket policy to the configured `<prefix>/sha256/*` namespace.
- Publication is a single-object PUT, never a multipart upload. It retries at
  most three times against the same content-addressed key, so a lost success
  response is idempotent. S3-compatible services must make a completed PUT
  visible to a subsequent HEAD; IronFlow verifies digest and size metadata
  before returning the descriptor.
- Failed or cancelled publication never returns a descriptor. S3's atomic PUT
  contract prevents a partial object from becoming visible, although an
  ambiguous completed PUT can remain as an unreferenced whole object and is
  handled by the offline prune command.
- Downloads enforce the configured ceiling and declared length while
  streaming, then verify SHA-256 before publishing or repairing the private
  cache. Cancellation drops the request and removes the private staging file.
- Pruning deletes one content-addressed object at a time. Object deletion is
  idempotent, so an interrupted sweep can be rerun; it does not provide a
  transaction across the candidate batch.

Handle-capable built-in consumers never receive the artifact store pathname.
For a third-party library that accepts only a path, the store can create a
private, random, read-only verified-path lease: it copies from the verified
handle with a byte limit and cancellation checkpoints, keeps the leased handle
open, and removes the pathname when the lease drops. The lease prevents store
path replacement from redirecting the library, but does not isolate a process
running under the same OS identity from mutating the leased inode.

## Concurrency Model

- Per-workflow task semaphore limits concurrent task executions
- Topological phase membership and member order are deterministic and follow
  flow declaration order. Context publication uses that order, not task finish
  order.
- Independent task events, logs, and external side effects retain their actual
  timing order and can vary between runs; event order does not define context
  collision precedence.
- Configurable via environment variable:
  - `IRONFLOW_MAX_CONCURRENT_TASKS` (default: num_cpus)
- Async cancellation drops the active node future. CPU-bound Lua work receives
  a cooperative cancellation flag while running on the blocking pool. ZIP
  traversal, parsing, compression, and copying use tracked blocking workers;
  they checkpoint between filesystem/archive entries and copied chunks so run
  and task admission remains held until physical work stops.
- Shell commands and persistent MCP stdio servers enable direct-child
  `kill_on_drop` on every platform. On Unix they also lead a process group.
  Closing an MCP session first closes stdin and gives the server time to exit;
  timeout, cancellation, or an unresponsive close invalidates the session and
  escalates termination of its owned process tree.
- MCP initialization returns a process-local opaque handle. A capacity-bound,
  idle-expiring registry owns both stdio and Streamable HTTP sessions; explicit
  `close` remains the normal lifecycle endpoint.

The deadline is an execution budget: task-state/event persistence required to
record the result is not forcibly interrupted by it. Cancellation cannot undo
an external side effect that already completed. Non-Unix descendant-tree
cleanup requires platform-specific job control and currently guarantees only
the direct child;
on Unix, a descendant that deliberately escapes its process group can outlive
the step. Custom blocking nodes must use the cooperative blocking helper and
checkpoints; the engine cannot forcibly unwind arbitrary synchronous Rust or a
third-party call.
