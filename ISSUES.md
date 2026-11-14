# IronFlow engineering issues

The canonical engineering ledger is maintained in
[`docs/issues/README.md`](docs/issues/README.md). Individual findings use
stable paths such as [`docs/issues/IF-001.md`](docs/issues/IF-001.md).

## Active findings

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
| [IF-194](docs/issues/IF-194.md) | P2 | Open | PDF rendering | pdf_to_image and pdf_thumbnail can bind Pdfium only once per process |
| [IF-199](docs/issues/IF-199.md) | P2 | Open | CLI/SQLite store | Default SQLite store URL breaks for store directories containing spaces, percent signs or question marks |
| [IF-200](docs/issues/IF-200.md) | P2 | Open | Artifact lifecycle | artifacts prune cannot get past a first page of still-referenced digests |
| [IF-201](docs/issues/IF-201.md) | P2 | Open | Scheduler/Redis | A Redis TTL shorter than a schedule's grace window re-runs a completed occurrence |
| [IF-202](docs/issues/IF-202.md) | P2 | Open | Storage/Postgres | PostgreSQL event store takes an exclusive table lock on every startup |
| [IF-203](docs/issues/IF-203.md) | P3 | Open | Maintainability | Assorted engine, API and documentation follow-ups from the November review |
| [IF-212](docs/issues/IF-212.md) | P2 | Open | Nodes/security | The outbound destination guard covers only the HTTP nodes |
| [IF-213](docs/issues/IF-213.md) | P2 | Open | Engine/cancellation | Several built-in blocking nodes are not tracked work, and Lua C calls have no time bound |
| [IF-214](docs/issues/IF-214.md) | P2 | Open | API events | Event streams can be lost when a delete overlaps a new run or incarnation admission fails |
| [IF-215](docs/issues/IF-215.md) | P2 | Open | Resource safety | Parsing, planning and fan-out paths that still do unbounded work |
| [IF-217](docs/issues/IF-217.md) | P3 | Open | Scheduler | Scheduler follow-ups: outcome labels, catch-up losses and cron messages |
| [IF-218](docs/issues/IF-218.md) | P3 | Open | File I/O | File output and archive follow-ups |
| [IF-219](docs/issues/IF-219.md) | P3 | Open | Diagnostics | Diagnostic scrubber over-redacts and misses some key forms |
| [IF-220](docs/issues/IF-220.md) | P3 | Open | Maintainability | Operational and validation follow-ups from the 2026-11 fix batches |
| [IF-221](docs/issues/IF-221.md) | P2 | Open | Workflow composition | Child input mappings and s3vector flags fall back silently on missing or malformed values |
| [IF-222](docs/issues/IF-222.md) | P3 | Open | Documents/extraction fidelity | Document extraction fidelity follow-ups for DOCX, PPTX and PDF metadata |
| [IF-223](docs/issues/IF-223.md) | P3 | Open | Data transforms | Data transform follow-ups: YAML non-finite and large numbers, CSV formula cells, neq on a missing field |

## Working agreement

1. Select one issue, or one tightly coupled pair, from the highest-priority
   active group and set its frontmatter status to `in-progress`.
2. Confirm the live code still supports the finding, then add focused
   regression coverage for the original defect or missing contract.
3. Align implementation, current documentation, and Lua examples.
4. During ordinary work, run focused tests and the required surface validators.
   Run the complete integration gate only at branch integration, before a
   `develop` push, during release preparation, or when explicitly requested.
5. Set an issue to `resolved` only after its acceptance criteria pass. Record
   the outcome, contract boundary, exact validation evidence, ISO completion
   date, and commit or PR when applicable.
6. Regenerate the indexes and run `bun run scripts/issues_registry.ts check`.

Historical audit baselines and cross-issue evidence are retained in
[`docs/issues/AUDIT_EVIDENCE.md`](docs/issues/AUDIT_EVIDENCE.md).
