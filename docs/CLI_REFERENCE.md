# IronFlow — CLI Reference

Complete reference for all commands, flags, and environment variables.

---

## Global Options

These options apply to all commands:

| Flag | Description |
|------|-------------|
| `--dotenv <PATH>` | Path to a `.env` file to load. If omitted, IronFlow checks only `.env` in the current working directory. |
| `-C, --config <PATH>` | Path to an `ironflow.yaml` configuration file. If omitted, IronFlow auto-detects `ironflow.yaml` in the current directory. |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

## Configuration Resolution

When a setting is available from more than one source, IronFlow uses this
deterministic order:

1. An explicit CLI argument, including an explicitly supplied value that is
   equal to the built-in default
2. An existing process environment variable
3. The selected dotenv file
4. `ironflow.yaml`
5. The built-in default

Only sources supported by a particular setting participate. For example,
`webhooks` is configuration-file-only, while `RUST_LOG` is environment-only.
An existing shell, container, or service environment variable is never
overwritten by dotenv. The dotenv file is parsed completely before any of its
values are installed, so a malformed file cannot leave a partially applied
environment.

A bootstrap CLI pass first validates the invocation and discovers
`--dotenv`. Dotenv is then resolved before tracing and the final source-aware
CLI parse. This makes dotenv values available to Clap-backed options, runtime
configuration, Lua `env()`, and `RUST_LOG`. Configuration-file discovery and
loading happen afterwards.

---

## Commands

### `ironflow run <FLOW>`

Execute a workflow from a Lua flow file.

| Argument / Flag | Required | Default | Description |
|-----------------|----------|---------|-------------|
| `<FLOW>` | yes | — | Path to the `.lua` flow file |
| `-c, --context <JSON>` | no | `{}` | Initial context as a JSON string |
| `-v, --verbose` | no | off | Show step details, per-task timing, and outputs |
| `--store-dir <DIR>` | no | `data/runs` | Directory for state persistence (`IRONFLOW_STORE_DIR`) |

```bash
ironflow run flow.lua --context '{"user": "Alice"}' --verbose
```

---

### `ironflow validate <FLOW>`

Parse and validate a flow file without executing it. Checks for:
- Unknown node types
- Missing or invalid dependencies
- DAG cycles
- Duplicate step names
- Invalid recovery targets, shared handlers, and recovery-graph cycles

| Argument / Flag | Required | Default | Description |
|-----------------|----------|---------|-------------|
| `<FLOW>` | yes | — | Path to the `.lua` flow file |

```bash
ironflow validate flow.lua
```

---

### `ironflow nodes`

List all registered node types with descriptions.

```bash
ironflow nodes
```

---

### `ironflow list`

List past workflow runs.

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `-s, --status <STATUS>` | no | all | Filter by status: `pending`, `running`, `success`, `failed`, `stalled`, `cancelled` |
| `--store-dir <DIR>` | no | `data/runs` | State store directory (`IRONFLOW_STORE_DIR`) |
| `--format <FORMAT>` | no | `table` | Output format: `table` or `json` |
| `--limit <COUNT>` | no | configured cap (`100`) | Records in this page; cannot exceed `IRONFLOW_MAX_LIST_RECORDS` |
| `--after <CURSOR>` | no | — | Continue after the opaque cursor printed/returned by the previous page |

```bash
ironflow list --status failed --format json
ironflow list --status failed --after <next_cursor> --format json
```

The command reads summaries rather than full contexts/task histories. JSON
output is a page envelope containing `runs`, `limit`, `returned`, `has_more`,
and `next_cursor`. There is intentionally no `--all` option. Ordering uses the
start timestamp at microsecond precision, newest first, puts missing timestamps
last, and uses descending run ID to break ties within the same microsecond.

This JSON shape is a next-major-version change. Earlier releases printed a
top-level array of full `RunInfo` objects, including context and task history;
consumers must now read summaries from the page envelope's `runs` field and use
`next_cursor` with `--after` for continuation. A filtered cursor is bound to
its filter, so every continuation command must repeat the same `--status`.

---

### `ironflow inspect <RUN_ID>`

Show full details for a specific run, including context, tasks, timing, and errors.

| Argument / Flag | Required | Default | Description |
|-----------------|----------|---------|-------------|
| `<RUN_ID>` | yes | — | A canonical run ID; generated IDs are UUIDv4 |
| `--store-dir <DIR>` | no | `data/runs` | State store directory (`IRONFLOW_STORE_DIR`) |

```bash
ironflow inspect 3362bbd5-429e-4860-893a-34b20f43b485
```

#### Run ID format

Engine-generated run IDs are UUIDv4 strings, but the accepted public and JSON
store format is a broader opaque ASCII token. Its exact input is not trimmed or
case-normalized:

- Length is 1 through 128 bytes.
- The first and last bytes are ASCII letters or digits.
- Interior bytes may be ASCII letters, digits, `-`, or `_`.

For example, `a`, `run-2026`, `tenant_1-run_42`, and a UUID are valid;
`-run`, `run_`, `run.id`, whitespace, Unicode, path separators, and traversal
forms are invalid. Public `GET /runs/{id}`, `DELETE /runs/{id}`, and
`GET /runs/{id}/events` requests validate the path value after percent decoding
and reject an invalid ID with HTTP `400` and code `bad_request` before
consulting a store. A well-formed ID that does not exist returns
`404 not_found`, except that `DELETE` returns success when it recovers an
orphaned retained event stream from an interrupted earlier deletion.

---

### `ironflow serve`

Start the REST API server.

| Flag | Required | Default | Env Var | Description |
|------|----------|---------|---------|-------------|
| `--host <HOST>` | no | `0.0.0.0` | `HOST` | Address to bind to |
| `-p, --port <PORT>` | no | `3000` | `PORT` | Port to listen on |
| `--store-dir <DIR>` | no | `data/runs` | `IRONFLOW_STORE_DIR` | State store directory |
| `--flows-dir <DIR>` | no | — | `FLOWS_DIR` | Directory for `.lua` flow files |
| `--max-body <BYTES>` | no | `1048576` | `MAX_BODY` | Maximum request body size in bytes |

Explicit CLI flags take precedence over environment variables, which take
precedence over matching `ironflow.yaml` fields and built-in defaults.
API authentication is required when binding to a non-loopback address. Set `IRONFLOW_API_KEY`; clients must send either `Authorization: Bearer <key>` or `X-API-Key: <key>`.
Browser CORS access is denied by default. Set `IRONFLOW_CORS_ORIGINS` or `cors_origins` in config to allow specific frontend origins.

```bash
# Local development
ironflow serve --host 127.0.0.1 --port 8080

# Docker / Railway / Fly.io (reads PORT from environment)
IRONFLOW_API_KEY="change-me" ironflow serve
```

### Configuration File

The `serve` command (and all other commands) can load settings from `ironflow.yaml`. Place it in the working directory for auto-detection, or specify a path with `-C`. Environment values, including values supplied by dotenv, override matching fields in this file:

```bash
ironflow -C /path/to/ironflow.yaml serve
```

#### Storage Backend

IronFlow supports these state storage backends:

- **json** (default) — File-based JSON storage in `store_dir`
- **sqlite** — SQL storage in a local SQLite database
- **postgres** — SQL storage in Postgres (requires a `-full` release binary or building with `--features postgres`)
- **redis** — Redis-backed storage (requires a `-full` release binary or building with `--features redis`)

The JSON backend accepts only the canonical run IDs above, confines record and
summary names to `store_dir`, and rejects a store root or run-related entry that
is a symbolic link instead of following it. Each main record and summary
sidecar is written through a temporary file in that same directory and then
committed as one filesystem operation, so readers do not observe a partially
written individual file. Initial main-record publication uses a same-directory
hard link and does not overwrite an existing main record; replacements and
summary commits rename a synced temporary file over their preflighted target.
On Unix, the containing directory is synced as well. The main record and
summary remain two separate file commits, not one transaction. Each carries
the same opaque revision and SHA-256 digest of the serialized public summary.
The main record is authoritative: listing trusts the sidecar only when a
bounded primary header matches both values and the sidecar content recomputes
to the digest. A missing, syntactically invalid, schema-unusable, or revision/
digest-mismatched cache falls back to a full primary decode and attempted
repair; a sidecar with an explicit string run ID that disagrees with its
filename is corruption and does not fall back. A failed sidecar commit or
repair is logged without hiding a successful primary mutation.

The bounded summary fast path does not read or decode the unused primary
suffix. A suffix-only corruption is therefore detected by full `inspect`/read
or mutation, not necessarily by summary listing; malformed headers and any
identity or digest mismatch reached by a full decode fail explicitly. Legacy
unversioned and revision-only primaries take the full-record path until a later
mutation upgrades both files.

Bounded pages use an immutable `.ironflow-run-catalog-v1.bin` fixed-record base
with one global and six status-specific ordered sections, plus a checksummed
`.ironflow-run-catalog-v1.delta` containing at most 128 coalesced run-ID
upserts/tombstones. The page path binary-searches the filter-bound cursor,
range-reads at most `limit + 1 + K` base entries, and merges the K-entry delta.
A clean page is therefore O(log N + page size + K), retains O(page size + K),
and does not enumerate `store_dir`, where `K <= 128`. A checksummed version-2
`.ironflow-run-catalog-v1.state` token binds the base generation and delta
revision; `.ironflow-run-catalog-v1.lock` coordinates local participating
writers. Initialization, status changes, and deletion normally replace only
the O(K) delta. The 129th distinct overlay ID compacts O(N) records into a new
base and empty delta, while repeated changes to one ID remain one entry.
Task/context-only updates keep both files unchanged. Missing, dirty, stale, or
malformed base/delta metadata is rebuilt lazily from authoritative main
records.

On Unix, IronFlow sets the JSON store directory to mode `0700` and committed
main, summary, catalog base, delta, state, and lock files to `0600`. Numeric
Unix mode guarantees do not apply on non-Unix platforms; configure equivalent
directory/file ACLs there. Atomic
publication/replacement is also subject to the underlying filesystem's
same-directory hard-link/rename semantics. The local catalog lock coordinates
participating store instances on one filesystem, but is neither distributed
coordination nor protection from a hostile external process. Prefer SQL or
Redis for sustained high-write or high-cardinality workloads.

Existing engine-created UUID filenames already satisfy the grammar. A
historical JSON entry created through direct store use with a noncanonical ID
is reported as corruption during listing rather than silently ignored. Migrate
such data offline: stop writers, make a backup, and either export/reimport it or
rename both main/summary files while updating their matching embedded IDs.
The same stopped-writer window can explicitly rebuild derived index metadata
with `JsonStateStore::rebuild_run_summary_catalog()`, which publishes a fresh
base and empty delta. Stop all writers before upgrading to or downgrading from
the version-2 catalog state format; mixed old/new writers are unsupported.
IronFlow refuses a symlink/non-regular base, delta, state, or lock entry; remove
such unsafe metadata offline before rebuilding it.

Configure via `ironflow.yaml`:

```yaml
store_backend: sqlite
store_url: "sqlite://data/runs/ironflow.sqlite?mode=rwc"
event_store: memory
event_memory_capacity: 10000
sql_table_prefix: "ironflow_"
```

Or via environment variables (override config file):

| Variable | Default | Description |
|----------|---------|-------------|
| `IRONFLOW_STORE` | `json` | Storage backend: `json`, `sqlite`, `postgres`, or `redis` |
| `IRONFLOW_STORE_URL` | SQLite auto path for `sqlite`; required for `postgres` | SQL store URL |
| `IRONFLOW_EVENT_STORE` | `memory` | Event backend for `/runs/{id}/events`: `memory`, `sqlite`, `postgres`, or `redis` |
| `IRONFLOW_EVENT_STORE_URL` | SQLite auto path for `sqlite`; required for `postgres` | SQL event store URL |
| `IRONFLOW_EVENT_MEMORY_CAPACITY` | `10000` | Positive global event/fence count when the event backend is `memory`; a fixed 64 MiB retained-heap estimate is enforced independently |
| `IRONFLOW_SQL_TABLE_PREFIX` | `ironflow_` | SQL table/index prefix for SQLite/Postgres state and event stores |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis connection URL |
| `REDIS_PREFIX` | `ironflow:` | Key prefix for Redis keys |
| `REDIS_TTL` | — | TTL seconds for Redis run/event keys and event-deletion fences: `1..=99,999,999,999` (no expiration if unset). State mutations and event publishes refresh their keys; the first successful event deletion sets the fence expiry, while same-owner retries preserve that original lifetime. |

When `IRONFLOW_STORE=sqlite` and no URL is configured, IronFlow creates `ironflow.sqlite` under `IRONFLOW_STORE_DIR`.
When `IRONFLOW_EVENT_STORE=sqlite` and no URL is configured, IronFlow creates `ironflow-events.sqlite` under `IRONFLOW_STORE_DIR`.
Run state and run events are configured separately, so a deployment can store run records in one backend and stream event replay from another. Redis event storage is available behind `--features redis` and uses `REDIS_URL`, `REDIS_PREFIX`, and optional `REDIS_TTL`.
Redis state mutations use revision-token compare-and-swap, while event payload/cursor publication is one idempotent Lua operation. These guarantees apply per operation; state and event writes are not a single transaction. The current multi-key layout supports standalone Redis, not Redis Cluster.

The Redis event compatibility path requires Redis 6.2 or newer for `LMOVE` and
uses a fixed internal policy; there is no configuration knob. An eligible
legacy family is atomically moved first into deterministic exact-run
quarantine. Migration then validates two full passes by reading at most 128
head elements and returning at most 1 MiB of serialized payload to Rust per
batch. Persisted generations and pending rotation intents make each same-list
head-to-tail `LMOVE` batch resumable without changing final event order.

One event-store operation confirms at most 32 bounded steps. If migration or
reverse restoration needs more work, it returns typed `Conflict`; HTTP surfaces
`409 conflict` and asks the operator to retry the same operation. Exact-run
progress survives requests and process restarts. A quarantined family whose
state is missing is reported as corruption and retained for manual recovery.

Alias-safe families can migrate automatically. For unsafe encoded run IDs, an
existing current family is accepted only with the exact owner marker; an
ambiguous ownerless current family requires manual migration. An optional raw
candidate migrates only when it carries the exact requested owner. Otherwise
it is preserved and ignored, and the collision-free encoded namespace remains
available.

Redis has no list-element length metadata, so it must read one oversized
element once before it can reject it. An element larger than 1 MiB, malformed
payload or owner, bad cursor index, or Rust decode failure triggers bounded
reverse rotations and restoration of the original family before corruption is
returned. Reverse windows enforce the same 128-element and 1 MiB bounds as
forward validation. Migration records the source's shortest TTL as an absolute
deadline and immediately aligns every quarantined component to it; renames and
rotations never refresh it, and restoration or finalization cannot extend
retention. Complete quarantine persisted past that deadline is expired, and
the next access removes its progress record after the governed namespaces are
gone. Partial families remain fail-closed. Capability probes use absent keys
and create no probe records.

Stop pre-protocol Redis event writers before migration. Recreation or mutation
of the source or destination while deterministic quarantine exists blocks
migration and preserves every family. Two matching digest scans are required,
and the last verification acknowledgement finalizes atomically; invalid data
is restored or blocked. Quarantine keys are internal protocol state and must
not be edited directly. IronFlow does not merge an old writer's recreated
source; stop that writer and resolve the collision before retrying.

Each steady-state Redis run-list request also inspects up to 32 catalog entries
through a persistent maintenance cursor whose cycle has a fixed high-water
member. This bounded incremental work cannot be starved by continuous newer
inserts: it eventually removes TTL-expired entries and revision-safely repairs
the full global/status catalog representation of every valid live member. A
revision conflict is deferred to a later bounded cycle rather than spinning on
a hot run. A missing or inconsistent derived catalog uses a one-time legacy
Set scan
protected by a renewable owner lease and a finalized generation marker; a page
rechecks that generation before returning so readers never accept a partial
rebuild.
SQL run deletion and SQL pruning wrap their run/task changes in transactions;
task upserts take the same per-run lock as deletion/pruning, and an error rolls
the complete operation back.

SQL event identity is `(run_id, id)`. Upgrading a table whose primary key is
the earlier global `(id)` form is a locked, guarded migration and requires a
coordinated stop of every older SQL event writer. Older binaries still use
`ON CONFLICT(id)` and fail after the migration. Once separate runs reuse an
event ID, downgrading requires an offline data transformation; the retained
legacy ordering index does not make a downgrade safe. SQLite rejects unknown
columns, uniqueness/index extensions, triggers, and foreign-key relationships
before rebuilding; PostgreSQL rejects extra uniqueness/exclusion constraints,
a deferrable primary key, and transactionally rolls back dependency failures.
Both dialects verify that the managed sequence-index name denotes the exact
live unique `(run_id, sequence)` index. Negative or trigger-altered counters and
no-progress legacy repairs fail as corruption instead of publishing an
unreadable or cursor-skipping event.
For shared SQL databases, set `IRONFLOW_SQL_TABLE_PREFIX` or `sql_table_prefix` to isolate IronFlow tables. Prefixes are strictly validated and may contain only ASCII letters, digits, and underscores after deriving table names.

Treat storage, event-store, SMTP, and webhook URLs as secrets when they contain
credentials. IronFlow diagnostics remove URL user information, redact every
query value, and discard fragments; webhook-style secret endpoints also hide
their paths. Malformed URLs fail closed to a redacted placeholder. This protects
current diagnostics, but cannot rewrite historical logs, so rotate credentials
and purge retained logs if an older version may have exposed a full URL.

GitHub releases include default binaries and `-full` binaries. Use the `-full` artifact when you need both Postgres and Redis support.

To build with optional backends locally:

```bash
cargo build --release --features postgres
cargo build --release --features redis
cargo build --release --features postgres,redis
```

#### CORS

By default, the API does not send `Access-Control-Allow-Origin`, so browser cross-origin requests are denied. Configure exact allowed origins in `ironflow.yaml`:

```yaml
cors_origins:
  - "https://app.example.com"
  - "https://admin.example.com"
```

Or via environment variable:

```bash
IRONFLOW_CORS_ORIGINS="https://app.example.com,https://admin.example.com" ironflow serve
```

Use `IRONFLOW_CORS_ORIGINS="*"` only when intentionally allowing any browser origin.

#### API Authentication

When the API is bound to a public interface, IronFlow requires an API key:

```bash
IRONFLOW_API_KEY="change-me" ironflow serve
```

Clients can authenticate with either header:

```bash
curl http://localhost:3000/runs \
  -H "Authorization: Bearer change-me"

curl http://localhost:3000/runs \
  -H "X-API-Key: change-me"
```

To intentionally run without API authentication, set `IRONFLOW_ALLOW_UNAUTHENTICATED_API=true` or `allow_unauthenticated_api: true` in config. A server whose resolved bind address is loopback (for example `127.0.0.1`, `localhost`, or `::1`) is allowed without a key for local development.

#### API Error Responses

Application errors use a JSON object with a human-readable `error` and a
stable `code`. Expected client and lookup failures preserve a safe message:

```json
{
  "error": "Run 'missing' not found",
  "code": "not_found"
}
```

Unexpected internal, storage-backend, and stored-data corruption failures
return HTTP `500` without exposing error chains, connection URLs, filesystem
paths, or credentials:

```json
{
  "error": "Internal server error",
  "code": "internal_error",
  "error_id": "6c783ea4-239f-4751-9cc6-b9e86e030eca"
}
```

The same UUID is returned in the `X-Error-ID` response header and recorded in
the server log for operator correlation. `details` is never returned for an
internal error. Invalid storage input, including a malformed public run ID,
maps to `400` with `bad_request`; missing records map to `404` with
`not_found`; and a concurrent storage conflict maps to `409` with `conflict`.
The run-event endpoint refines a missing supplied replay cursor to `410` with
`event_cursor_gone`, because the run still exists but that replay position is
no longer available.

#### Run Listing

`GET /runs` returns lightweight run summaries ordered by `started DESC NULLS
LAST, id DESC`, after normalizing start times to UTC microsecond precision.
Runs whose timestamps differ only below one microsecond therefore use
descending run ID as their tie-breaker. The endpoint accepts an optional
`status` filter, `limit`, and an opaque `after` cursor. The ordinary default
limit is 50. The hard maximum is `IRONFLOW_MAX_LIST_RECORDS` (default 100), and
a smaller configured maximum also lowers the default. Zero or over-limit
requests return `400 bad_request`; `offset` is rejected with guidance to use
`after`.

The response contains `runs`, `limit`, `returned`, `has_more`, and
`next_cursor`. A cursor is bound to the status filter used to create it, so it
cannot accidentally continue a different result set. Listing has no
unbounded mode and does not compute an exact catalog total.

```bash
curl 'http://127.0.0.1:3000/runs?status=failed&limit=25'
curl 'http://127.0.0.1:3000/runs?status=failed&limit=25&after=<next_cursor>'
```

##### Next-major-version migration boundary

The run-list redesign is intentionally breaking and must ship on a major
version boundary. Migrate all coupled surfaces together:

| Surface | Earlier contract | Cursor-page contract |
|---------|------------------|----------------------|
| HTTP query | `status`, `limit`, `offset` | `status`, `limit`, `after`; any `offset` is rejected |
| HTTP response | `runs`, exact `total`, `limit`, `offset`, `returned` | `runs`, `limit`, `returned`, `has_more`, `next_cursor`; exact `total` is removed |
| CLI JSON | top-level array of full run records | page envelope of lightweight summaries |
| Rust store trait | summary listing could fall back to an unbounded vector method | every `StateStore` implementor must provide `list_run_summaries_page(&RunListQuery)` |
| Embedded API setup | `AppState` / `ServeOptions` had no listing policy | constructors must supply a validated `ListingPolicy` |

There is no compatibility `all` switch. To consume the complete history,
follow `next_cursor` one bounded page at a time. The default hard cap is 100
records per page and can be changed only through
`IRONFLOW_MAX_LIST_RECORDS`.

#### Run Deletion

`DELETE /runs/{id}` deletes the state record first, then idempotently deletes
all retained events for that run and installs a fence against late event
publication. If event cleanup fails after state deletion, retry the same
request: IronFlow still attempts event cleanup for the now-missing state and
returns success when it removes an orphaned stream. A missing state record with
no orphaned events returns `404 not_found`.

SQL deletion fences are durable. A Redis fence is persistent without
`REDIS_TTL` and expires with that configured TTL. The memory backend keeps
events and fences in one oldest-first queue bounded by both the configured
entry count and a fixed 64 MiB retained-heap estimate, so they disappear on
restart or eviction. Current-layout Redis deletion atomically fences and
unlinks its list/index/sequence/layout keys after preflighting the command.
If `UNLINK` is unavailable or denied by Redis ACLs, deletion fails before any
fence or namespace mutation.
An eligible unmarked legacy stream first moves into deterministic quarantine
and completes the bounded rotational validation described above. If migration
or reverse restoration returns `409 conflict` after its 32-step budget, retry
the same `DELETE`: state remains absent, saved exact-run progress resumes, and
the lifecycle coordinator can finish orphan event cleanup. A validation fault
restores the original family and fails deletion; an ambiguous unsafe family or
orphaned deterministic snapshot requires manual recovery. No deletion fence
or event namespace is removed until the family is owner-marked and current.
The Rust `StateStore` methods remain state-only; embedded callers that need
this coordinated behavior should use `storage::lifecycle::delete_run`.

#### Run Events

`GET /runs/{id}/events` streams compact run/task lifecycle events as Server-Sent Events. Events include run/task status, step name, node type, attempts, timing, errors, and skip reasons, but never full node input/output.

```bash
curl -N http://localhost:3000/runs/<run_id>/events \
  -H "Authorization: Bearer change-me"
```

Use `?after=<event_id>` to replay events after a known event cursor.
Failures discovered during run and event-store preflight, before the SSE
response starts, use the normal JSON error contract above.

##### Replay and failure contract

Replay and failure behavior is a stable client contract:

- Event IDs are opaque cursor tokens. Every logical event must have a non-empty
  ID that is never assigned to another event in the same run, even after
  retention or TTL expiry; engine-generated UUIDv4 event IDs satisfy this
  obligation. An exact publication retry is idempotent while the backend
  retains that identity. Bounded and TTL-backed stores can detect conflicting
  ID reuse only while the prior identity remains retained. A cursor is
  exclusive: the cursor event is not repeated, and events after it are emitted
  in the selected backend's stable read order. The server drains every event
  in a fetched batch before reading again and does not drop the rest of a page
  after its first item. A reconnect is therefore replayable, but consumers
  should still de-duplicate by event ID if their own processing and cursor
  persistence are not atomic.
  Memory and Redis replay append order. SQL resolves the opaque event ID to a
  transactionally allocated, monotonic per-run publication sequence, so its
  replay order no longer depends on the event timestamp or UUID. Legacy SQL
  rows are assigned sequences in 256-row transaction batches using their
  former `(timestamp, id)` order.
- Same-phase task lifecycle events preserve actual publication/completion
  timing and can vary between runs. They do not define context precedence;
  buffered context output is committed separately in flow declaration order.
- A non-empty `Last-Event-ID` request header is the effective cursor and takes
  precedence over `?after=`. This is required for browser `EventSource`
  reconnects when the URL still contains an older bootstrap query cursor. If
  the header is absent or empty, a non-empty `after` value is used; otherwise
  replay starts at the oldest retained event. Multiple `Last-Event-ID` fields,
  a non-UTF-8 header value, or a cursor containing NUL, carriage return, or
  newline is rejected with `400 bad_request`.
- The initial event-store read happens before the `200 text/event-stream`
  response is committed. A missing run remains `404 not_found`. A supplied
  cursor that is unknown, belongs to another run, or is no longer retained is
  intentionally indistinguishable and returns `410 Gone` with code
  `event_cursor_gone`. Other initial storage failures use the normal typed JSON
  error contract, including the correlated generic `500` response for backend
  or corruption failures.
- The `run_finished` event is emitted once and then the server closes the
  stream after flushing it. A connection resumed at the terminal cursor closes
  successfully without repeating that event. If the authoritative run state
  is already terminal but no terminal event is retained, the server drains the
  available replay and closes without fabricating an event.
- Once SSE headers have been sent, an error cannot become a different HTTP
  status. A polling/storage failure instead emits one ID-less `stream_error`
  event with safe JSON data and closes the stream. Storage failures use code
  `event_stream_error` and correlate an opaque `error_id` with sanitized logs.
  Only a failure classified as `Backend` sets `retryable: true`; corruption
  and conflict set it to `false`, because reconnecting cannot repair durable
  data.
  If the active cursor disappeared, the expected client condition instead uses
  `event_cursor_gone`, sets `retryable: false`, and omits `error_id`. No SSE ID
  is attached in any case, so the last domain-event cursor remains available
  for reconnect.
- A domain-event serialization failure follows the same close path with code
  `event_serialization_error`, `retryable: false`, and a correlated internal
  `error_id`. It never emits an empty placeholder and never advances the
  cursor.
- While no events are available for a non-terminal run, the server waits one
  second between event-store polls. While a fetched batch is buffered, it emits
  without that polling delay. After 15 seconds without an event frame, a
  `: keep-alive` SSE comment maintains the transport; comments carry no ID, do
  not change replay position, and stop when the stream closes.

An in-stream storage failure will have this shape (the UUID is illustrative):

```text
event: stream_error
data: {"error":"Event stream unavailable","code":"event_stream_error","error_id":"550e8400-e29b-41d4-a716-446655440000","retryable":true}
```

#### Webhook Routes

Define webhook-to-flow mappings in `ironflow.yaml` to expose flows as named HTTP endpoints:

```yaml
flows_dir: "data/flows"

webhooks:
  hello: hello_world.lua  # shorthand: no request headers reach the flow
  signed-order:
    flow: orders/process.lua
    forward_headers:
      - x-webhook-signature
```

- Flow paths are resolved relative to `flows_dir`
- POST only — JSON body becomes initial workflow context
- Request headers are denied by default; only names in `forward_headers` are
  exposed through lowercase `ctx._headers`
- Forwarded header values are confidential, execution-only inputs: they are
  removed/redacted before context, task, event, and run-detail persistence
- `Authorization`, `X-API-Key`, cookies, proxy credentials, and common
  platform/session credential headers cannot be forwarded to a workflow
- A forwarded value must contain at least eight non-whitespace bytes. Repeated
  or non-text values fail the request with `400` instead of becoming ambiguous
- Webhook name is injected as `ctx._webhook`
- Request bodies cannot define the reserved `_headers`, `_webhook`, or
  `_flow_dir` context keys

```bash
curl -X POST http://localhost:3000/webhooks/signed-order \
  -H "X-API-Key: $IRONFLOW_API_KEY" \
  -H "X-Webhook-Signature: $WEBHOOK_SHARED_SECRET" \
  -H "Content-Type: application/json" \
  -d '{"order_id": "123"}'
```

The API key authenticates the caller to IronFlow and is consumed by the API
middleware. The separately configured business signature is available only
while workflow nodes execute. Workflow code should validate it in place and
return a boolean/result, never log, return, encode, or otherwise copy the raw
value. Literal copies are redacted, but deliberately transformed or externally
transmitted secrets cannot be identified reliably.

Webhook bodies are parsed JSON; IronFlow does not expose the original request
bytes needed by provider schemes that sign an exact raw payload. Verify those
schemes in an upstream gateway until a raw-body verification contract exists.

Runs created by older IronFlow versions may already contain request headers.
Public run output hides legacy `_headers` values when they are still present,
but operators should purge old run/event data and rotate credentials that were
previously sent to webhook endpoints.

---

## Environment Variables

### CLI and storage

These configure `serve` and, where applicable, other commands that open the
state store. Explicit CLI flags override them; they override matching
configuration-file fields. `IRONFLOW_STORE_DIR` applies to `run`, `list`,
`inspect`, and `serve`.

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Server bind address |
| `PORT` | `3000` | Server listen port |
| `IRONFLOW_STORE_DIR` | `data/runs` | State store directory |
| `IRONFLOW_STORE` | `json` | State store backend: `json`, `sqlite`, `postgres`, or `redis` |
| `IRONFLOW_STORE_URL` | SQLite auto path for `sqlite`; required for `postgres` | SQL store URL |
| `IRONFLOW_EVENT_STORE` | `memory` | Event backend for `/runs/{id}/events`: `memory`, `sqlite`, `postgres`, or `redis` |
| `IRONFLOW_EVENT_STORE_URL` | SQLite auto path for `sqlite`; required for `postgres` | SQL event store URL |
| `IRONFLOW_EVENT_MEMORY_CAPACITY` | `10000` | Positive global event/fence count when the event backend is `memory`; a fixed 64 MiB retained-heap estimate applies independently; zero or invalid values fail startup |
| `IRONFLOW_SQL_TABLE_PREFIX` | `ironflow_` | SQL table/index prefix for SQLite/Postgres state and event stores |
| `FLOWS_DIR` | — | Flow files directory |
| `MAX_BODY` | `1048576` | Max request body size (bytes) |
| `IRONFLOW_API_KEY` | — | API key required for non-loopback API servers |
| `IRONFLOW_ALLOW_UNAUTHENTICATED_API` | `false` | Explicitly allow unauthenticated API access |
| `IRONFLOW_CORS_ORIGINS` | — | Comma-separated allowed browser origins; use `*` to allow any origin |

### Run listing

This is resolved after dotenv loading by both `serve` and `list`.

| Variable | Default | Description |
|----------|---------|-------------|
| `IRONFLOW_MAX_LIST_RECORDS` | `100` | Positive hard cap for each API/CLI run-list page; invalid or zero values fail startup/listing |

### Engine

| Variable | Default | Description |
|----------|---------|-------------|
| `IRONFLOW_MAX_CONCURRENT_TASKS` | number of CPUs | Maximum tasks running in parallel per workflow execution |
| `IRONFLOW_MAX_CONCURRENT_RUNS` | unlimited | Process-wide cap on concurrently-executing API/webhook-triggered runs. When at capacity, new run requests receive `503 Service Unavailable`. `0` or unset means unlimited. |
| `IRONFLOW_MAX_RUN_SECONDS` | unlimited | Run-level wall-clock deadline. A run exceeding it is cancelled (terminal status `cancelled`), reclaiming a step that hangs without its own `timeout()`. `0` or unset means no deadline. |
| `IRONFLOW_LUA_MAX_INSTRUCTIONS` | `5000000` | Max Lua VM instructions per flow parse/code execution; `0` disables |
| `IRONFLOW_LUA_MAX_SECONDS` | `10` | Max wall-clock seconds per Lua state; `0` disables |
| `IRONFLOW_LUA_MAX_MEMORY_BYTES` | `134217728` | Max Lua VM memory per Lua state; `0` disables |
| `IRONFLOW_LUA_HOOK_INTERVAL` | `10000` | Instruction interval for budget checks |
| `IRONFLOW_LUA_GC_AFTER_EXECUTION` | `true` | Run a Lua garbage-collection cycle after flow parsing/code execution |
| `IRONFLOW_CACHE_MAX_ENTRIES` | `10000` | Max entries retained by the process-global `cache_set` / `cache_get` memory backend |
| `IRONFLOW_CACHE_DIR` | `.ironflow_cache` | Default directory for the `cache_set` / `cache_get` file backend when `cache_dir` is not set |
| `IRONFLOW_DB_MAX_ROWS` | `1000` | Max rows returned by `db_query`; `0` disables |
| `IRONFLOW_DB_MAX_RESULT_BYTES` | `10485760` | Max serialized JSON result size for `db_query`; `0` disables |
| `IRONFLOW_LLM_MAX_RESPONSE_BYTES` | `26214400` | Max LLM provider response body size; `0` disables |
| `IRONFLOW_MAX_HTTP_BODY_BYTES` | `52428800` | Maximum HTTP node response-body size |
| `IRONFLOW_MAX_FILE_BYTES` | `52428800` | Maximum `read_file` / `write_file` payload size |
| `IRONFLOW_MAX_SHELL_OUTPUT_BYTES` | `10485760` | Maximum captured bytes for each shell output stream and each MCP stdio JSON-RPC frame |
| `IRONFLOW_MAX_TASK_OUTPUT_BYTES` | `2097152` | Maximum serialized task output persisted in run state before replacement with a truncation marker |
| `IRONFLOW_MAX_DIRECTORY_ENTRIES` | `10000` | Maximum entries returned by a directory listing |
| `IRONFLOW_MAX_DIRECTORY_DEPTH` | `32` | Maximum recursive directory traversal depth |
| `IRONFLOW_MAX_ZIP_ENTRIES` | `10000` | Maximum ZIP entries processed by archive nodes |
| `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | `536870912` | Maximum total uncompressed bytes processed by archive nodes |
| `IRONFLOW_MAX_PDF_BYTES` | `104857600` | Maximum PDF file size accepted by rendering nodes |
| `IRONFLOW_MAX_PDF_RENDER_PAGES` | `25` | Maximum pages rendered by one PDF node call |
| `IRONFLOW_MAX_PDF_RENDER_PIXELS` | `25000000` | Maximum pixels in one rendered PDF page |
| `IRONFLOW_MAX_PDF_DPI` | `300` | Maximum PDF rendering DPI |
| `IRONFLOW_MAX_DETACHED_SUBWORKFLOWS` | `64` | Process-wide limit for detached `subworkflow` executions |
| `IRONFLOW_MCP_SESSION_CACHE_SIZE` | `1024` | Maximum live MCP session handles; least-recently-used overflow sessions are closed |
| `IRONFLOW_MCP_SESSION_TTL_SECS` | `3600` | Idle TTL for live MCP sessions; expired sessions are closed when another session is inserted or leased |
| `IRONFLOW_OAUTH_CACHE_SIZE` | `128` | Maximum cached OAuth client token tuples |

Lua limits apply to flow parsing, `code` nodes, and `foreach` transform functions. For trusted dedicated-server workloads that intentionally run long Lua computations, raise the budgets or set the relevant budget to `0`.

### Diagnostics

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | `tracing` filter; dotenv is loaded before the tracing subscriber is initialized |

### Dotenv

IronFlow resolves at most one dotenv file at startup:

- Without `--dotenv`, it checks exactly `.env` in the current working directory.
  Absence is silently accepted; parent directories are not searched.
- With `--dotenv <PATH>`, it uses exactly that path. A missing, unreadable, or
  malformed explicit file is a startup error.
- A discovered default file must also be readable and valid. IronFlow parses
  the complete file before changing the process environment.
- Existing process variables win over duplicate dotenv keys. Within the dotenv
  file, the first declaration of a duplicated key wins.

The merged values are available to CLI configuration, tracing, runtime limits,
and Lua flows via the `env()` function:

```lua
local api_key = env("API_KEY")
```

### User-defined variables

Any variable in the final merged process environment is accessible from Lua
flows via `env("KEY")`. Common patterns:

```bash
# .env
OPENAI_API_KEY=sk-...
DATABASE_URL=postgres://localhost/mydb
SLACK_WEBHOOK=https://hooks.slack.com/...
```

```lua
flow:step("call", nodes.http_post({
    url = "https://api.openai.com/v1/chat/completions",
    auth = { type = "bearer", token = env("OPENAI_API_KEY") }
}))
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Error (flow load failure, validation error, execution failure) |
