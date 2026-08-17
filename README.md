# IronFlow — Workflow Automation Engine in Rust

**A lightweight, high-performance workflow engine built in Rust with Lua scripting. The open-source alternative to n8n, Airflow, and Prefect that ships as a single binary without a Python, Node.js, or container runtime.**

IronFlow is a DAG-based workflow orchestration engine designed for CI/CD pipelines, data processing, ETL jobs, API integrations, document extraction, and task automation. Define workflows in simple Lua scripts — a language with only ~20 keywords — and run them on a sandboxed, async Rust runtime with parallel step execution, retry logic, conditional routing, and a built-in REST API.

No Python. No Node.js. No Docker required. Most workflows need only the IronFlow binary; the `pdf_to_image` and `pdf_thumbnail` nodes additionally require a native Pdfium library. IronFlow runs on Linux, macOS, edge servers, and air-gapped environments.

```lua
local flow = Flow.new("process_order")

flow:step("validate", nodes.validate_schema({
    source_key = "order",
    schema = {
        type = "object",
        required = { "order_id", "amount" },
        properties = {
            order_id = { type = "string" },
            amount = { type = "number" }
        }
    }
}))

flow:step("charge", nodes.http_post({
    url = "https://payments.example.com/charge",
    body = { amount = "${ctx.order.amount}", order_id = "${ctx.order.order_id}" },
    auth = { type = "bearer", token = env("PAYMENT_API_KEY") }
})):depends_on("validate"):retries(3, 1.0)

flow:step("notify", nodes.log({
    message = "Order ${ctx.order.order_id} charged successfully",
    level = "info"
})):depends_on("charge")

return flow
```

```bash
ironflow run process_order.lua --context '{"order": {"order_id": "ORD-42", "amount": 99.99}}'
```

---

## Why IronFlow?

**For teams that can't use n8n, Dify, or hosted workflow platforms** — whether due to company policy, air-gapped environments, or the need for something faster and simpler.

IronFlow gives you a workflow engine that:

- **Ships as a single binary** — no Python, Node.js, package manager, or container runtime required; PDF rendering nodes require Pdfium
- **Runs anywhere** — Linux, macOS, CI/CD pipelines, edge servers, embedded systems
- **Is fast** — Rust-powered execution with parallel step scheduling via DAG resolution
- **Is safe** — Sandboxed Lua can't access the filesystem or OS unless you explicitly allow it
- **Is easy to learn** — Lua has ~20 keywords. If you can write JSON, you can write flows

## The Architecture

Rust as the runtime + Lua as the scripting layer. A well-proven pattern used by Neovim, OpenResty/Nginx, Redis, and game engines like Roblox.

```
┌─────────────────────────────────────────────────────────┐
│                     Lua Flow Scripts                     │
│  flow:step("name", nodes.http_get({...}))               │
│  flow:step("process", function(ctx) ... end)            │
├─────────────────────────────────────────────────────────┤
│                    IronFlow Engine                        │
│  DAG resolution · Parallel execution · Retry/timeout     │
│  Context propagation · Conditional routing · State store │
├─────────────────────────────────────────────────────────┤
│                   102 Built-in Nodes                       │
│  HTTP · Files · Shell · Transforms · Conditionals · ...  │
│  All implemented in pure Rust for performance & safety   │
└─────────────────────────────────────────────────────────┘
```

| What | How |
|------|-----|
| Flow definitions | Lua scripts — easy to write, read, and modify |
| Node implementations | Pure Rust — fast, memory-safe, no GC pauses |
| Shared context | Lua table backed by Rust HashMap, serialized as JSON |
| DAG resolution | Topological sort with cycle detection (Kahn's algorithm) |
| Parallel execution | Steps without dependencies run concurrently via Tokio |
| State persistence | Rust trait with JSON and SQLite backends; optional PostgreSQL and Redis features |

## Features

- **103 built-in nodes** — HTTP (GET/POST/PUT/DELETE), file I/O, ZIP utilities (`zip_create`, `zip_list`, `zip_extract`), S3 operations, shell commands, JSON/CSV/XML/YAML transforms, foreach iteration, key-value caching (memory + file), conditional routing, schema validation, hashing, templating, Markdown conversion, HTML sanitization, document extraction (Word/PDF/PPTX/HTML/VTT/SRT/Excel), PDF merge/split, database queries (SQLite via sqlx, ArangoDB via HTTP), AI text embeddings/chunking (`ai_*`) and chat/completions (`llm`) across providers, audio/video transcription (`transcribe`) via OpenAI, OpenAI-compatible, or Azure, an MCP 2025-11-25 client over persistent stdio and Streamable HTTP (`mcp_client`), notification integrations (`send_email`, `slack_notification`), data extraction helpers (`json_extract_path`, `if_body_contains`, `if_http_status`), delays, inline code execution, bounded repeated subworkflow composition, LLM tool dispatch (`tool_dispatch`), presigned S3 URL support, base64 encoding/decoding, date formatting, image helpers (`pdf_to_image`, `pdf_thumbnail`, `image_to_pdf`, `image_resize`, `image_crop`, `image_rotate`, `image_flip`, `image_grayscale`, `image_metadata`, `image_convert`, `image_watermark`, `pdf_metadata`).
- **Function handlers** — pass Lua functions directly as step handlers, no boilerplate needed
- **Conditional step shorthand** — `step_if(condition, name, handler)` for concise branching
- **DAG-based scheduling** — steps run in parallel unless dependencies are declared
- **Retry with exponential backoff** — configurable per step
- **Total per-step timeouts** — one budget across attempts and retry backoff,
  with process-group cleanup on Unix
- **Conditional routing** — `if_node` and `switch_node` for branching workflows
- **Context interpolation** — documented node parameters support explicit context paths such as `${ctx.user.name}`, `${ctx.items[0].name}`, and `${ctx["key.with.dots"]}`
- **Lua globals** — `env()`, `uuid4()`, `now_rfc3339()`, `now_unix_ms()`, `json_parse()`, `json_stringify()`, `log()`, `base64_encode()`, `base64_decode()`
- **Schema validation** — JSON Schema validation to fail fast on bad input
- **REST API** — run and manage flows over HTTP (Axum-based)
- **CLI** — run, validate, inspect, and list workflows from the terminal
- **Planned error recovery** — `on_error` schedules a dedicated recovery step
  inside the validated DAG, with normal dependencies, retries, and timeouts
- **Subworkflow composition** — call reusable `.lua` flows once, fan out runtime work with `parallel_subworkflows`, or iterate explicit state with bounded `repeat_subworkflow`
- **Sandboxed execution** — Lua scripts run without `os`, `io`, or `debug` access

## Quick Start

### Build

```bash
git clone https://github.com/skitsanos/ironflow.git
cd ironflow
cargo build --release
```

### Run a flow

```bash
# Simple flow
./target/release/ironflow run examples/01-basics/hello_world.lua --context '{"user_name": "Alice"}'

# Call OpenAI and extract the reply with a function handler
./target/release/ironflow run examples/05-http/openai_with_extract.lua --context '{"prompt": "Explain recursion"}'

# Validate without executing
./target/release/ironflow validate examples/03-control-flow/switch_routing.lua

# Verbose mode — see per-task timing and outputs
./target/release/ironflow run examples/07-advanced/data_pipeline.lua --verbose
```

### Start the REST API

```bash
./target/release/ironflow serve --host 127.0.0.1 --port 3000
```

```bash
# Run a flow via API
curl -X POST http://127.0.0.1:3000/flows/run \
  -H "Content-Type: application/json" \
  -d '{
    "source": "local flow = Flow.new(\"hello\")\nflow:step(\"greet\", nodes.log({ message = \"Hello ${ctx.user}!\" }))\nreturn flow",
    "context": {"user": "Alice"}
  }'

# Or send base64-encoded Lua to avoid JSON escaping
curl -X POST http://127.0.0.1:3000/flows/run \
  -H "Content-Type: application/json" \
  -d '{
    "source_base64": "bG9jYWwgZmxvdz1GbG93Lm5ldygiaGVsbG8iKTtmbG93OnN0ZXAoImdyZWV0Iixub2Rlcy5sb2coe21lc3NhZ2U9IkhlbGxvICR7Y3R4LnVzZXJ9ISJ9KSk7cmV0dXJuIGZsb3c=",
    "context": {"user": "Alice"}
  }'
```

## Configuration

For settings with multiple sources, IronFlow resolves values as explicit CLI
argument > existing process environment > selected dotenv file >
`ironflow.yaml` > built-in default. An explicit CLI value still wins when it is
equal to the default, and dotenv never replaces a variable already supplied by
the shell, container, or service manager.

Without `--dotenv`, IronFlow checks only `.env` in the current working
directory and silently continues when it is absent. An explicit dotenv file,
or a discovered default file that exists, must be readable and valid. The
whole file is parsed before any values are applied, tracing, or the final
source-aware CLI parse, so dotenv can configure `RUST_LOG`, CLI environment aliases,
runtime limits, and Lua `env()` calls without partial startup state.

The repository's [`.env.example`](.env.example) contains safe starter values.
See the [CLI reference](docs/CLI_REFERENCE.md#configuration-resolution) for the
complete contract and configuration variables.

Extraction CPU, RSS, output, persistence, concurrency, and cancellation-drain
trends can be measured with the opt-in
[release subprocess benchmark](docs/EXTRACTION_BENCHMARK.md). Local calibration
documents stay in ignored `data/samples/`; benchmark results are not timing
gates for ordinary CI.

## CLI Commands

| Command | Description |
|---------|-------------|
| `ironflow run <file>` | Execute a workflow |
| `ironflow validate <file> [--strict]` | Validate a flow without running; strict mode rejects Lua handler warnings |
| `ironflow nodes` | List all available node types |
| `ironflow list` | List a bounded page of past workflow runs (`--limit`, `--after`) |
| `ironflow inspect <run_id>` | Inspect a specific run |
| `ironflow artifacts prune` | Offline bounded cleanup of artifacts not referenced by retained runs |
| `ironflow serve` | Start the REST API server |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/flows/run` | Execute a flow; optional `Idempotency-Key` makes retries converge on one durable run |
| `POST` | `/flows/validate` | Validate a flow |
| `GET` | `/runs` | List run summaries with `status`, `limit`, and `after` cursor parameters |
| `GET` | `/runs/{id}` | Get run details |
| `GET` | `/runs/{id}/events` | Stream run/task lifecycle events over SSE |
| `DELETE` | `/runs/{id}` | Delete run state and retained events (`409` while a non-terminal owner lease is live) |
| `GET` | `/nodes` | List available nodes |
| `POST` | `/webhooks/{name}` | Execute a webhook-mapped flow |
| `GET` | `/health` | Backwards-compatible liveness check |
| `GET` | `/health/live` | Process liveness |
| `GET` | `/health/ready` | Admission and durable-store readiness |

Flows can also run on a schedule. A `schedules:` block in `ironflow.yaml`
declares bounded standard five-field cron triggers that `ironflow serve`
evaluates—in a named time zone, with at most one replica claiming a given
instant. If day-of-month and weekday are both restricted, either match fires
the schedule, as in traditional crontab. A claimed instant may still
be skipped for lateness, overlap, or capacity; overlap suppression is a bounded
best-effort check rather than a distributed lock. Schedule names evaluate
concurrently under a 15-second per-tick budget, and the scheduler task is
supervised with the API server: an unexpected scheduler exit stops `serve`
instead of leaving an apparently healthy API with no triggers. JSON and SQL
claim retention is schedule-scoped, cadence-limited, and capped at 256 records
per cleanup pass; Redis uses per-claim TTL. See
[Schedules](docs/CLI_REFERENCE.md#schedules).

Active-active deployments must set `IRONFLOW_REPLICA_MODE=true` and use
PostgreSQL or Redis for both state and events. SIGTERM closes readiness and new
execution admission before a bounded drain. See the
[replica deployment contract](docs/REPLICA_DEPLOYMENT.md) and its opt-in
two-process Docker fault gate.

Handler failures return JSON with `error` and a stable `code`. Internal failures
return only a generic message plus an opaque `error_id`, also exposed as
`X-Error-ID` for correlation with sanitized server logs. Full error chains and
connection credentials are never included in the response.

Public run IDs are opaque 1–128 byte ASCII tokens: they must start and end with
a letter or digit, and may contain letters, digits, `-`, or `_` between them.
IronFlow-generated IDs are UUIDv4 strings. Malformed IDs are rejected with
`400 bad_request`; reading a valid but unknown ID returns `404 not_found`.
Deleting one also returns `404` unless retained orphan events are recovered.

Run listings are ordered by start time at microsecond precision, newest first,
with missing timestamps last and descending run ID as the deterministic
tie-breaker. Both the API and CLI enforce `IRONFLOW_MAX_LIST_RECORDS` (default
`100`); there is no unbounded `all` mode. The API normally returns 50 records
(or the configured cap when lower), and `next_cursor` / `--after` reaches the
following page.

The cursor listing contract is an intentional next-major-version boundary.
`GET /runs` replaces `offset` with `after` and no longer returns an exact
`total`; `ironflow list --format json` now returns a summary page envelope
instead of an array of full run records. API, CLI, and Rust-library consumers
must migrate together. See the [CLI reference](docs/CLI_REFERENCE.md#run-listing)
for the exact old-to-new mapping.

The default JSON state backend rejects symlinked store roots, run records, and
summary entries, and publishes each file through a same-directory temporary file.
The main record is authoritative: main and summary files carry the same opaque
revision and SHA-256 digest of the public summary. Listing accepts a sidecar
only when a bounded primary header matches both values and the sidecar content
recomputes to that digest. Missing, syntactically invalid, schema-unusable, or
revision/digest-mismatched sidecars fall back to the full primary and are
best-effort repaired; a sidecar with an explicit string run ID that disagrees
with its filename is corruption and does not fall back. The bounded fast path
intentionally does not decode the remainder of a primary whose committed header
and sidecar agree; full reads and mutations still validate the complete primary
record.
Bounded pages use an immutable checksummed fixed-record base with global and
per-status sections plus a checksummed, coalesced delta capped at 128 distinct
run IDs. Clean cursor pages binary-search the base, range-read the requested
window plus the bounded overlay, and merge both without enumerating the store
directory: O(log N + page size + K) reads and O(page size + K) memory, where
`K <= 128`. Participating local writers mark the projection dirty before a
primary change and coordinate through one file lock. Initialization, status
changes, and deletion normally replace only the O(K) delta; the 129th distinct
overlay ID performs an O(N) compaction into a new base and resets the delta.
Task/context-only updates leave both files unchanged. A version-2 clean token
binds the base generation and delta revision, so missing, dirty, stale, or
malformed metadata rebuilds automatically from authoritative primaries. Stop
all writers when upgrading or downgrading across this state format. For an
explicit offline repair and compaction, stop writers and call
`JsonStateStore::rebuild_run_summary_catalog()`. The JSON backend remains a
moderate-cardinality local store; prefer SQL or Redis for sustained high-write
workloads.

On Unix, its directory is mode `0700` and committed files are `0600`; other
platforms require equivalent operator-managed ACLs. See the precise filesystem
and atomicity boundary in
[Architecture](docs/ARCHITECTURE.md#json-store-filesystem-boundary).

Run-event SSE supports an exclusive `?after=<event_id>` cursor and standard
`Last-Event-ID` reconnection, drains complete backend pages, and closes after
the terminal event. Failures found before streaming use the JSON contract
above; failures after `200` use one ID-less `stream_error` frame and then close
without advancing the replay cursor. See the complete client contract in the
[CLI reference](docs/CLI_REFERENCE.md#run-events).

Every logical event must have a non-empty ID that is never assigned to another
event in the same run; the engine-generated UUIDv4 IDs satisfy this obligation.
An exact publication retry is idempotent while the backend retains that event
identity. Bounded or TTL-backed stores can detect conflicting ID reuse only
while the prior identity remains retained. Unknown, expired, and cross-run
event cursors return storage `NotFound` and become `410 event_cursor_gone` at
the HTTP boundary.

The default in-memory event backend retains events and deletion fences in one
oldest-first queue across all runs. It enforces both a 10,000-entry limit and a
fixed 64 MiB retained-heap estimate so large metadata cannot bypass the count
bound; set `IRONFLOW_EVENT_MEMORY_CAPACITY` or `event_memory_capacity` to
another positive entry limit. `DELETE /runs/{id}` removes state first, then
idempotently removes retained events and fences late event publication.
Repeating the request can finish event cleanup left by an interrupted first
attempt. A non-terminal run with an unexpired execution-owner lease is left
intact and returns `409 conflict`; terminal and expired-lease runs are
deletable.

Redis legacy event migration requires Redis 6.2 or newer. It atomically moves
an eligible family into deterministic exact-run quarantine, then validates it
twice through head batches of at most 128 events and 1 MiB returned to Rust.
Persisted generations and pending intents make same-list `LMOVE` rotations
resumable without changing final event order. One operation confirms at most
32 bounded steps before returning a typed conflict that asks the caller to
retry. Interrupted requests and processes resume saved progress; a missing
state record leaves the deterministic snapshot intact and fails closed.

Unsafe run IDs are handled automatically only with an exact owner marker: an
encoded current family is accepted, while an owned raw family can migrate.
An ambiguous ownerless encoded family requires manual recovery; an optional
raw candidate without that exact owner is preserved and ignored. Redis must
read one oversized list element once because it has no
element-length metadata; validation then fails closed and boundedly restores
the original family. Forward and reverse batches both enforce the 1 MiB
aggregate limit. The shortest source TTL is applied to every quarantined
component immediately; expired quarantine is released on the next access
without reviving retained events. Stop pre-protocol event writers during
migration. See the full ownership, recovery, and TTL boundaries in
[Architecture](docs/ARCHITECTURE.md).

SQL event IDs are scoped to their run. Upgrading a legacy SQL event table whose
primary key is only `id` requires a coordinated stop of older event writers;
after `(run_id, id)` permits cross-run ID reuse, downgrading needs an offline
data transformation. See the storage upgrade boundary in
[Architecture](docs/ARCHITECTURE.md).

## Writing Flows

### Steps and dependencies

```lua
local flow = Flow.new("my_pipeline")

-- Steps without dependencies run in parallel
flow:step("fetch_users", nodes.http_get({
    url = "https://api.example.com/users",
    output_key = "users"
}))
flow:step("fetch_orders", nodes.http_get({
    url = "https://api.example.com/orders",
    output_key = "orders"
}))

-- This step waits for both
flow:step("merge", nodes.log({
    message = "Got users ${ctx.users_data} and orders ${ctx.orders_data}"
})):depends_on("fetch_users", "fetch_orders")

return flow
```

Each DAG phase reads one immutable phase-start context snapshot, so independent
steps cannot consume one another's output. Their outputs are committed after
the phase settles, in flow declaration order; if two steps publish the same
key, the later-declared step wins. Use `depends_on()` when data must flow from
one step to another, and distinct `output_key` prefixes when parallel results
must coexist. Parallel event, log, and external-side-effect order remains
timing-dependent.

### Context interpolation

Node documentation identifies which string parameters support context
interpolation. Supported parameters use path lookup, not Lua expressions:

```lua
nodes.template_render({
    template = 'User ${ctx.user.name} chose ${ctx.items[0].name} (${ctx["display.label"]})',
    output_key = "summary"
})
```

Array indexes are zero-based, including in Lua flow strings. Dot notation is
for identifier-like object keys; JSON double-quoted bracket notation accesses
keys containing dots or other punctuation. Function calls, operators, and
fallback expressions are not part of the grammar. Compute those values in an
explicit function/code step first.

Missing and `null` values render as an empty string. Other `${...}` forms such
as `${HOME}` remain literal for tools such as shells to interpret. To emit a
literal `${ctx.value}`, prefix it with a backslash at runtime; in a Lua string,
write `"\\${ctx.value}"`.

### Function handlers

Write inline Lua logic as step handlers — no need for `nodes.code()`:

```lua
flow:step("transform", function(ctx)
    local total = 0
    for _, item in ipairs(ctx.items) do
        total = total + item.price * item.qty
    end
    return { order_total = total }
end):depends_on("load_items")
```

Handlers are serialized into an isolated Lua VM. Keep their local state inside
the function or pass values through `ctx`; captured outer locals are rejected.
Validation reports undefined handler and string-backed `code` globals as
source-positioned warnings, and `ironflow validate flow.lua --strict` treats
those warnings as failures. It also compiles string-backed code during
validation, without executing it, so invalid syntax fails immediately.

### Retries and timeouts

```lua
flow:step("call_api", nodes.http_post({
    url = "https://unreliable-api.example.com/submit",
    body = { data = "${ctx.payload}" },
    timeout = 10
})):retries(3, 1.0):timeout(30)
-- 3 retries with 1s → 2s → 4s exponential backoff
-- 30s total execution budget across all attempts and backoff
```

The step timeout does not reset for each retry. When it expires, the active
async node future is dropped, `code`/`foreach` and nested-flow Lua evaluation
are interrupted through their execution hooks, owned shell subprocesses are
terminated, and an active MCP session is invalidated and cleaned up. MCP stdio
cleanup also stops its owned server process. Durable state writes used to
record the outcome are not forcibly interrupted by that deadline. A timeout
cannot roll back an external side effect that already completed; see
[Lua Flow Guide — Timeout](docs/LUA_FLOW_GUIDE.md#timeout) for the exact
cancellation boundary.

### Conditional routing

```lua
-- Simple: step_if runs the step only when the condition is true
flow:step_if("ctx.amount > 100", "vip_discount", nodes.code({
    source = "return { discount = ctx.amount * 0.1 }"
}))

-- Full control: if_node + route for multi-branch workflows
flow:step("check", nodes.if_node({
    condition = "ctx.amount > 100",
    true_route = "premium",
    false_route = "standard"
}))

flow:step("premium_flow", nodes.log({
    message = "VIP: $${ctx.amount}"
})):depends_on("check"):route("premium")

flow:step("standard_flow", nodes.log({
    message = "Standard: $${ctx.amount}"
})):depends_on("check"):route("standard")
```

## Built-in Nodes

| Category | Nodes |
|----------|-------|
| **HTTP** | `http_request`, `http_get`, `http_post`, `http_put`, `http_delete` |
| **Files** | `read_file`, `write_file`, `copy_file`, `move_file`, `delete_file`, `list_directory` |
| **S3** | `s3_presign_url`, `s3_get_object`, `s3_put_object`, `s3_delete_object`, `s3_copy_object`, `s3_list_objects`, `s3_list_buckets` |
| **S3 Vectors** | `s3vector_create_bucket`, `s3vector_get_bucket`, `s3vector_delete_bucket`, `s3vector_create_index`, `s3vector_get_index`, `s3vector_delete_index`, `s3vector_put_vectors`, `s3vector_query_vectors`, `s3vector_delete_vectors` |
| **Shell** | `shell_command` |
| **Transforms** | `json_parse`, `json_stringify`, `json_extract_path`, `csv_parse`, `csv_stringify`, `select_fields`, `rename_fields`, `data_filter`, `data_transform`, `batch`, `deduplicate`, `foreach` |
| **Conditionals** | `if_node`, `if_body_contains`, `if_http_status`, `switch_node` |
| **Validation** | `validate_schema`, `json_validate` |
| **Markdown** | `markdown_to_html`, `html_to_markdown` |
| **Cache** | `cache_set`, `cache_get` |
| **Notification** | `send_email`, `slack_notification` |
| **Database** | `db_query`, `db_exec`, `arangodb_aql` |
| **Composition** | `subworkflow`, `parallel_subworkflows`, `repeat_subworkflow`, `tool_dispatch`, `code` |
| **XML** | `xml_parse`, `xml_stringify` |
| **YAML** | `yaml_parse`, `yaml_stringify` |
| **HTML** | `html_sanitize` |
| **Encoding** | `base64_encode`, `base64_decode` |
| **Date/Time** | `date_format` |
| **Utility** | `log`, `hash`, `delay`, `template_render` |
| **ZIP** | `zip_create`, `zip_list`, `zip_extract` |
| **MCP** | `mcp_client` |
| **AI** | `ai_embed`, `ai_chunk`, `ai_chunk_merge`, `ai_chunk_semantic`, `llm`, `transcribe` |
| **Extraction** | `extract_word`, `extract_pdf`, `extract_pptx`, `extract_html`, `extract_vtt`, `extract_srt`, `extract_xlsx`, `pdf_to_image`, `pdf_thumbnail`, `pdf_metadata`, `image_to_pdf`, `pdf_merge`, `pdf_split` |
| **Image Processing** | `image_resize`, `image_crop`, `image_rotate`, `image_flip`, `image_grayscale`, `image_metadata`, `image_convert`, `image_watermark` |

See [docs/NODE_REFERENCE.md](docs/NODE_REFERENCE.md) for the complete reference with parameters and examples.

## Examples

Progressive examples from basic to advanced:

| Folder | What you'll learn |
|--------|-------------------|
| [01-basics](examples/01-basics/) | Logging, context passing, parallel execution, retries, env vars, Lua globals |
| [02-data-transforms](examples/02-data-transforms/) | JSON parse/stringify, filtering, batching, deduplication |
| [03-control-flow](examples/03-control-flow/) | If/else routing, switch/case routing, step_if shorthand |
| [04-file-operations](examples/04-file-operations/) | Read, write, copy, move, delete, list |
| [05-http](examples/05-http/) | API calls, authentication, OpenAI integration |
| [06-shell](examples/06-shell/) | Shell commands with args, env vars, timeouts, and explicit exit-status policy |
| [07-advanced](examples/07-advanced/) | Hashing, schema validation, full data pipelines, function handlers, base64 encoding |
| [08-extraction](examples/08-extraction/) | Word/PDF/PPTX/HTML/VTT/SRT/Excel extraction, metadata, PDF-to-image rendering, image resize/crop, PDF merge/split, image metadata |
| [09-cache](examples/09-cache/) | In-memory and file-based key-value caching with TTL |
| [10-database](examples/10-database/) | SQLite CRUD operations with db_query and db_exec |
| [11-subworkflow](examples/11-subworkflow/) | Subworkflow composition, fire-and-forget, on_error handling |
| [12-arangodb](examples/12-arangodb/) | ArangoDB AQL queries with bind variables and env-based credentials |
| [13-ai](examples/13-ai/) | Text embeddings (OpenAI, Ollama, OAuth), text chunking (fixed, split, merge, semantic) |
| [14-notifications](examples/14-notifications/) | Email via Resend or SMTP, Slack webhooks |
| [15-webhooks](examples/15-webhooks/) | Config-driven webhook routes with default-deny, execution-only signature headers |
| [16-s3vector](examples/16-s3vector/) | S3 Vectors RAG pipelines, transcript indexing with time-anchored chunks, similarity search, metadata filtering, query expansion |
| [17-mcp](examples/17-mcp/) | Stateful MCP 2025-11-25 workflows over stdio and Streamable HTTP, including explicit session cleanup |
| [18-xml-yaml](examples/18-xml-yaml/) | XML and YAML parsing/stringifying |
| [19-html-sanitize](examples/19-html-sanitize/) | HTML sanitization with configurable allowed tags |
| [20-date](examples/20-date/) | Date parsing, formatting, and timezone conversion |

Fixture-backed examples use the compact, synthetic, CC0 inputs in
[`examples/fixtures`](examples/fixtures/). The exhaustive
[`examples/catalog.json`](examples/catalog.json) records each flow's execution
category and composable external-service, credential, local-state, and platform
requirements. Review the [requirements and effects
legend](examples/README.md#requirements-and-effects) before execution. For an
experiment, pass `--store-dir` an empty disposable directory to isolate
IronFlow's run records; this does not redirect node output or undo remote
mutations. The fixture-backed offline subset runs from an isolated working
directory in CI.

## Development validation and versioning

Routine work uses focused tests for the behavior changed. Rust changes also
require formatting, the module-size policy, and
`cargo clippy --all-targets -- -D warnings`; the whole suite is intentionally
not run on each save or ordinary commit.

Enable the repository hooks once per checkout:

```bash
git config --local core.hooksPath .githooks
```

Before a `develop` push, the pre-push hook fails closed unless the worktree is
clean, no open pull request targets `develop`, remote `develop` is integrated,
and the committed version is a new `X.Y.Z-dev.N`. It then runs the full local
integration gate, including disposable Redis and PostgreSQL tests. Start or
advance a candidate with:

```bash
bun run scripts/development_version.ts bump minor  # 1.15.0 -> 1.16.0-dev.1
bun run scripts/development_version.ts bump next   # 1.16.0-dev.1 -> dev.2
```

CI runs the full suite on pushes to `develop` and `main`, with optional manual
dispatch. Its Linux release build is passed directly to Lua example validation;
the example job does not wait for macOS or compile a second release binary.
Default Clippy/tests and combined PostgreSQL/Redis feature checks each share a
single Linux workspace, avoiding isolated check and per-backend compilations.
Container publication uses a version- and digest-pinned Rust/cargo-chef builder
so source and package-version changes retain the dependency layer, backed by a
dedicated zstd-compressed GHCR BuildKit cache manifest. The mutable
`buildcache-amd64` tag is build input only; deploy the commit-tagged application
image by its immutable digest. Release promotion creates
`release/X.Y.Z` from verified `develop`, finalizes the candidate there with
`bun run scripts/development_version.ts finalize`, and merges that exact
candidate into `main` before the stable tag. Stable versions never land on
`develop`.

## Roadmap

The maintained [`Now / Next / Later` roadmap](docs/ROADMAP.md) separates
committed work from candidates and records IronFlow's enterprise deployment
boundaries. The current priority is a bounded production metrics contract; the
streamed S3-compatible artifact lifecycle is now part of the delivered
baseline. A read-only Web UI for flow and run visualization remains a later
candidate rather than an implementation commitment.

## License

MIT — see [LICENSE](LICENSE).
