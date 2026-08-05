# IronFlow issue registry

This is the canonical registry for IronFlow engineering findings. Each finding
has a stable page whose frontmatter is the source of truth for status,
priority, area, and title. The registry is generated with
`bun run scripts/issues_registry.ts generate` and verified with
`bun run scripts/issues_registry.ts check`.

- Total findings: 101
- Active findings: 1
- Historical audit evidence: [AUDIT_EVIDENCE.md](./AUDIT_EVIDENCE.md)

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
| [IF-001](./IF-001.md) | P0 | Resolved | Lua security | Package loader restores removed OS/I/O libraries |
| [IF-002](./IF-002.md) | P0 | Resolved | Runtime safety | Invalid numeric configuration panics or deadlocks |
| [IF-003](./IF-003.md) | P0 | Resolved | Runtime safety | Lua-to-JSON cycles can abort the process |
| [IF-004](./IF-004.md) | P0 | Resolved | Engine | Runs can remain `running` after internal failure/cancellation |
| [IF-005](./IF-005.md) | P0 | Resolved | Storage | Redis state/event mutations are not atomic |
| [IF-006](./IF-006.md) | P1 | Resolved | Engine | Step timeout is not a total/preemptive deadline |
| [IF-007](./IF-007.md) | P1 | Resolved | Engine | `on_error` bypasses DAG semantics and can erase failure state |
| [IF-008](./IF-008.md) | P1 | Resolved | Lua DSL | `step_if(...):depends_on(...)` evaluates its guard too early |
| [IF-009](./IF-009.md) | P1 | Resolved | CLI | Failed workflows exit with status code 0 |
| [IF-010](./IF-010.md) | P1 | Resolved | MCP | Stdio lifecycle/JSON-RPC validation do not support real servers |
| [IF-011](./IF-011.md) | P1 | Resolved | API security | Webhooks persist credentials and cookies in run context |
| [IF-012](./IF-012.md) | P1 | Resolved | API events | SSE drops batches, hides errors, and never terminates |
| [IF-013](./IF-013.md) | P1 | Resolved | API security | Internal errors and connection URLs can leak secrets |
| [IF-014](./IF-014.md) | P1 | Resolved | Storage security | JSON run IDs/permissions need hardening |
| [IF-015](./IF-015.md) | P1 | Resolved | Text processing | Fixed AI chunking can corrupt UTF-8 |
| [IF-016](./IF-016.md) | P1 | Resolved | Examples | Clean checkout lacks fixtures used by 40 examples |
| [IF-017](./IF-017.md) | P2 | Resolved | API/CLI | Documented loopback hosts do not all bind |
| [IF-018](./IF-018.md) | P2 | Resolved | CLI config | Dotenv/config precedence is inconsistent |
| [IF-019](./IF-019.md) | P2 | Resolved | Engine | Parallel context-key collisions are nondeterministic |
| [IF-020](./IF-020.md) | P2 | Resolved | Storage | Summaries, deletes, and event retention can drift |
| [IF-021](./IF-021.md) | P2 | Resolved | API/storage | Run pagination loads the full catalog |
| [IF-022](./IF-022.md) | P2 | Resolved | Nodes | Shell failure discards documented structured output |
| [IF-023](./IF-023.md) | P2 | Resolved | Interpolation | Examples use unsupported expressions/array paths |
| [IF-024](./IF-024.md) | P2 | Resolved | Docs | Node counts, backends, APIs, and signatures have drifted |
| [IF-025](./IF-025.md) | P2 | Resolved | Quickstart | README commands/API payloads are not runnable as written |
| [IF-026](./IF-026.md) | P2 | Resolved | Examples | Examples overwrite inputs or retain machine-specific state |
| [IF-027](./IF-027.md) | P2 | Resolved | Features | PostgreSQL disabled-feature errors are unclear |
| [IF-028](./IF-028.md) | P3 | Resolved | Architecture | Large/duplicated modules need bounded extraction |
| [IF-029](./IF-029.md) | P3 | Resolved | JSON storage | Run pages required full filesystem catalog scans |
| [IF-030](./IF-030.md) | P2 | Resolved | Redis events | Legacy event adoption/deletion is bounded and resumable |
| [IF-031](./IF-031.md) | P2 | Resolved | S3 Vectors | Examples cannot delete indexes or vector buckets |
| [IF-032](./IF-032.md) | P2 | Resolved | S3 Vectors | Resource targets can mix explicit and environment identifiers |
| [IF-033](./IF-033.md) | P3 | Resolved | JSON storage | Projection-changing writes replace the complete run catalog |
| [IF-034](./IF-034.md) | P3 | Resolved | Architecture | Module-size policy has no automated regression guard |
| [IF-035](./IF-035.md) | P0 | Resolved | Lua security | Flow-controlled Lua bytecode is loaded in binary (`bt`) mode |
| [IF-036](./IF-036.md) | P0 | Resolved | Runtime safety | Extract/encoding/S3 reads bypass `IRONFLOW_MAX_*` byte limits |
| [IF-037](./IF-037.md) | P0 | Resolved | Runtime safety | Deeply nested XML/YAML overflows the stack (uncatchable) |
| [IF-038](./IF-038.md) | P1 | Resolved | API DoS | Parse-time Lua pattern backtracking pins runtime workers |
| [IF-039](./IF-039.md) | P1 | Resolved | Nodes/security | `db_query`/`db_exec` interpolate ctx into `AssertSqlSafe` query text |
| [IF-040](./IF-040.md) | P1 | Resolved | Supply chain | Vulnerable dependencies (RUSTSEC) with no CI audit gate |
| [IF-041](./IF-041.md) | P1 | Resolved | Nodes/security | `http_request` allows SSRF via ctx URL and unrestricted redirects |
| [IF-042](./IF-042.md) | P2 | Resolved | Engine | `IRONFLOW_MAX_CONCURRENT_TASKS` is per-run, not process-wide |
| [IF-043](./IF-043.md) | P2 | Resolved | Engine | No crash/restart reconciliation; runs stay `Running` forever |
| [IF-044](./IF-044.md) | P2 | Resolved | API security | API key compared in non-constant time |
| [IF-045](./IF-045.md) | P2 | Resolved | API security | Flow-load parse errors disclose arbitrary local-file contents |
| [IF-046](./IF-046.md) | P2 | Resolved | Engine | `max_retries` is effectively unbounded (retry storm) |
| [IF-047](./IF-047.md) | P2 | Resolved | Engine | Dropped `RunHandle` cannot cancel a hung untimed node |
| [IF-048](./IF-048.md) | P2 | Resolved | Engine | Task-output cap does not bound shared/final context |
| [IF-049](./IF-049.md) | P2 | Resolved | Nodes | `read_file` size guard bypassed for special files |
| [IF-050](./IF-050.md) | P2 | Resolved | Nodes | `base64_decode` performs unbounded arbitrary-path writes |
| [IF-051](./IF-051.md) | P2 | Resolved | Storage | `prune_before` default loads the full catalog into memory |
| [IF-052](./IF-052.md) | P3 | Resolved | Maintainability | Assorted consistency/operability follow-ups |
| [IF-053](./IF-053.md) | P2 | Resolved | Nodes | `subworkflow` error propagation is implicitly coupled to `output_key` |
| [IF-054](./IF-054.md) | P2 | Resolved | API | `/flows/run` always accepts inline flow source, so an API key implies arbitrary execution |
| [IF-055](./IF-055.md) | P1 | Resolved | Storage | Concurrent SQL schema creation crash-loops a replica on first start |
| [IF-056](./IF-056.md) | P2 | Resolved | CLI config | `IRONFLOW_ALLOW_ADHOC_FLOWS` parses leniently and fails open on an unrecognized value |
| [IF-057](./IF-057.md) | P2 | Resolved | Nodes | `_`-prefixed context keys are not private when a child result is namespaced |
| [IF-058](./IF-058.md) | P2 | Resolved | Lua runtime | Conversion ceilings have no environment override and fail with an unactionable error |
| [IF-059](./IF-059.md) | P1 | Resolved | Scheduler | No way to run a flow on a schedule |
| [IF-060](./IF-060.md) | P1 | Resolved | Nodes | No way to read a spreadsheet |
| [IF-061](./IF-061.md) | P1 | Resolved | API security | Flow admission and HTTP redirect trust boundaries are incomplete |
| [IF-062](./IF-062.md) | P1 | Resolved | Engine/storage | Run ownership and crash recovery are not replica-safe |
| [IF-063](./IF-063.md) | P1 | Resolved | Resource safety | Transcription, S3, and XLSX work bypass end-to-end ceilings |
| [IF-064](./IF-064.md) | P1 | Resolved | ZIP/security | ZIP work outlives cancellation and extraction follows destination symlinks |
| [IF-065](./IF-065.md) | P1 | Resolved | Extraction/runtime | Non-XLSX extractors block async workers and lack end-to-end limits |
| [IF-066](./IF-066.md) | P1 | Resolved | Engine/resource safety | Extraction output and ZIP metadata amplify before memory caps apply |
| [IF-067](./IF-067.md) | P2 | Resolved | Tooling/performance | Extraction resource behavior has no repeatable benchmark harness |
| [IF-068](./IF-068.md) | P1 | Resolved | Artifact security | Local artifact reads trust a mutable pathname and do not verify content identity |
| [IF-069](./IF-069.md) | P1 | Resolved | Binary/PDF safety | Legacy file materialization and PDF merge amplify before their limits apply |
| [IF-070](./IF-070.md) | P2 | Resolved | Release governance | `main` requires a check that cannot run before merge |
| [IF-071](./IF-071.md) | P2 | Resolved | Test reliability | Cancellation cleanup test signals completion before staging cleanup |
| [IF-072](./IF-072.md) | P2 | Resolved | Test reliability | Lease-loss test can race its first heartbeat under load |
| [IF-073](./IF-073.md) | P1 | Resolved | Scheduler availability | A hung evaluation or dead scheduler task silently stops every schedule |
| [IF-074](./IF-074.md) | P1 | Resolved | Scheduler contract | Schedule configuration and cron semantics are not bounded or portable |
| [IF-075](./IF-075.md) | P2 | Resolved | Scheduler performance | Schedule claim cleanup is linear on every fire |
| [IF-076](./IF-076.md) | P1 | Resolved | Replica availability | Replica survival has no real-process Docker acceptance gate |
| [IF-077](./IF-077.md) | P1 | Resolved | Deployment lifecycle | Serve lacks draining, readiness, and restricted-container support |
| [IF-078](./IF-078.md) | P1 | Resolved | Durable admission | Retried submissions and schedule claim-to-run gaps can duplicate or lose work |
| [IF-079](./IF-079.md) | P1 | Resolved | Railway deployment | Replica lifecycle has no Railway canary evidence |
| [IF-080](./IF-080.md) | P1 | Resolved | OpenShift deployment | Restricted-SCC and Route behavior lack live platform evidence |
| [IF-081](./IF-081.md) | P1 | Resolved | Container delivery | Hosted canaries build or deploy mutable images instead of a verified registry digest |
| [IF-082](./IF-082.md) | P2 | Resolved | Tooling/resource use | Repeated integration gates retain tens of gigabytes of stale linked artifacts |
| [IF-083](./IF-083.md) | P2 | Resolved | Container performance | Package-version and source changes invalidate every Rust dependency layer |
| [IF-084](./IF-084.md) | P2 | Resolved | CI performance | Example validation waits for macOS and compiles the Linux release binary twice |
| [IF-085](./IF-085.md) | P2 | Resolved | Hosted acceptance | CI consolidation and container caching lack hosted timing evidence |
| [IF-086](./IF-086.md) | P2 | Resolved | CI performance | Independent checks and storage jobs compile the same Rust graph repeatedly |
| [IF-087](./IF-087.md) | P2 | Resolved | Container performance | Warm dependency reuse transfers a 755 MB gzip layer |
| [IF-088](./IF-088.md) | P2 | Resolved | Test reliability | Detached run-admission cleanup uses an undersized hosted-runner deadline |
| [IF-089](./IF-089.md) | P2 | Resolved | Release performance | Tag-scoped Windows caches force both release variants to rebuild dependencies |
| [IF-090](./IF-090.md) | P1 | Resolved | Dependency security | Informational advisories do not fail CI and unused defaults retain unsafe or abandoned crates |
| [IF-091](./IF-091.md) | P1 | Resolved | PDF/resource safety | PDF extraction reparses input and retains the final dependency advisory |
| [IF-092](./IF-092.md) | P2 | Resolved | PDF/performance | PDF resource improvements lack matched before/after RSS and timing evidence |
| [IF-093](./IF-093.md) | P2 | Resolved | PDF/correctness | Non-ASCII CID glyph extraction depends only on ignored local samples |
| [IF-094](./IF-094.md) | P2 | Resolved | Test reliability | Lease-loss regression still races hosted-runner scheduling |
| [IF-095](./IF-095.md) | P1 | Resolved | Lua correctness | Validation misses undefined globals and captured locals in Lua handlers |
| [IF-096](./IF-096.md) | P1 | Resolved | Lua correctness | String-backed code silently resolves undefined globals to nil |
| [IF-097](./IF-097.md) | P2 | Resolved | Test reliability | Default CI contains two timing-dependent test failures |
| [IF-098](./IF-098.md) | P1 | Resolved | API security | Webhook authentication cannot verify signatures over the original body |
| [IF-099](./IF-099.md) | P2 | Resolved | Product governance | Roadmap has no actionable forward plan and the shipped baseline has drifted |
| [IF-100](./IF-100.md) | P1 | Open | Observability | Production deployments lack a bounded metrics contract |
| [IF-101](./IF-101.md) | P1 | Resolved | Artifact lifecycle | Artifact storage has no remote multi-host lifecycle backend |
