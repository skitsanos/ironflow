# IronFlow engineering issues

The canonical engineering ledger is maintained in
[`docs/issues/README.md`](docs/issues/README.md). Individual findings use
stable paths such as [`docs/issues/IF-001.md`](docs/issues/IF-001.md).

## Active findings

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
| [IF-177](docs/issues/IF-177.md) | P1 | Open | Runtime safety | Diagnostic redaction is quadratic on async workers and misses common credential shapes |
| [IF-178](docs/issues/IF-178.md) | P1 | Open | PDF/resource safety | extract_pdf font decoding is unbounded for wide ToUnicode ranges and long glyph strings |
| [IF-179](docs/issues/IF-179.md) | P1 | Open | Files/write safety | Image nodes and transcribe write outputs through symlinks without the staged writer |
| [IF-181](docs/issues/IF-181.md) | P1 | Open | Storage | A context nested 126 levels deep makes a run record unreadable and blocks serve startup |
| [IF-182](docs/issues/IF-182.md) | P2 | Open | Scheduler availability | Schedules due in the same tick lose occurrences to the flow-load limit |
| [IF-183](docs/issues/IF-183.md) | P2 | Open | Deployment lifecycle | New connections hang for the whole drain and readiness never reports draining |
| [IF-184](docs/issues/IF-184.md) | P2 | Open | API events | A deleted run's event fence mutes a re-created run with the same ID |
| [IF-185](docs/issues/IF-185.md) | P2 | Open | Files/write_file | Staged writers publish mode 0600 and write_file append loses concurrent writes |
| [IF-186](docs/issues/IF-186.md) | P2 | Open | Interpolation | File-source helper interpolates twice and lets write_file artifact inputs name local paths |
| [IF-187](docs/issues/IF-187.md) | P2 | Open | Lua DSL | step_if and if_node comparisons against true, false and nil are always false |
| [IF-188](docs/issues/IF-188.md) | P2 | Open | Workflow composition | Child workflows have no nesting limit and outlive a timed-out parent step |
| [IF-189](docs/issues/IF-189.md) | P2 | Open | Resource safety | s3_list_objects, json_parse and task error text materialize without a bound |
| [IF-190](docs/issues/IF-190.md) | P2 | Open | ZIP/admission | ZIP preflight and the archive reader can select different end records |
| [IF-191](docs/issues/IF-191.md) | P2 | Open | Nodes/security | block_private_network passes CGNAT, NAT64 and other internal address ranges |
| [IF-192](docs/issues/IF-192.md) | P2 | Open | S3 Vectors | s3vector_query_vectors runs unfiltered when filter_key does not resolve |
| [IF-193](docs/issues/IF-193.md) | P2 | Open | Documents/extraction fidelity | Document extraction garbles PDF metadata and drops DOCX paragraph text around text boxes |
| [IF-194](docs/issues/IF-194.md) | P2 | Open | PDF rendering | pdf_to_image and pdf_thumbnail can bind Pdfium only once per process |
| [IF-195](docs/issues/IF-195.md) | P2 | Open | Nodes | date_format ignores input offsets; date_format and html_sanitize panics stall the run |
| [IF-196](docs/issues/IF-196.md) | P2 | Open | Data transforms | xml_stringify writes keys as markup; CSV type inference and data_filter equality lose values |
| [IF-197](docs/issues/IF-197.md) | P2 | Open | AI nodes | llm sends unknown providers to OpenAI and ignores documented parameters |
| [IF-198](docs/issues/IF-198.md) | P2 | Open | API security | An empty IRONFLOW_API_KEY disables the configured API key on loopback |
| [IF-199](docs/issues/IF-199.md) | P2 | Open | CLI/SQLite store | Default SQLite store URL breaks for store directories containing spaces, percent signs or question marks |
| [IF-200](docs/issues/IF-200.md) | P2 | Open | Artifact lifecycle | artifacts prune cannot get past a first page of still-referenced digests |
| [IF-201](docs/issues/IF-201.md) | P2 | Open | Scheduler/Redis | A Redis TTL shorter than a schedule's grace window re-runs a completed occurrence |
| [IF-202](docs/issues/IF-202.md) | P2 | Open | Storage/Postgres | PostgreSQL event store takes an exclusive table lock on every startup |
| [IF-203](docs/issues/IF-203.md) | P3 | Open | Maintainability | Assorted engine, API and documentation follow-ups from the November review |

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
