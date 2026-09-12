# IronFlow engineering issues

The canonical engineering ledger is maintained in
[`docs/issues/README.md`](docs/issues/README.md). Individual findings use
stable paths such as [`docs/issues/IF-001.md`](docs/issues/IF-001.md).

## Active findings

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
| [IF-109](docs/issues/IF-109.md) | P1 | Open | Database correctness | SQLite numeric expressions silently become null |
| [IF-110](docs/issues/IF-110.md) | P1 | Open | Image/resource safety | Image resize omits a potentially dominant intermediate allocation |
| [IF-111](docs/issues/IF-111.md) | P1 | Open | PDF/resource safety | Initial PDF loading leaves object-stream decompression uncapped |
| [IF-112](docs/issues/IF-112.md) | P1 | Open | PDF/confidentiality | A one-page PDF split retains unselected pages' objects |
| [IF-113](docs/issues/IF-113.md) | P2 | Open | XML/OOXML fidelity | XML and OOXML extraction lose entities, CDATA, and split text |
| [IF-114](docs/issues/IF-114.md) | P2 | Open | PDF/page fidelity | PDF split/merge discard inherited page attributes |
| [IF-115](docs/issues/IF-115.md) | P2 | Open | PPTX/page fidelity | PPTX extraction uses filenames instead of presentation order and membership |
| [IF-116](docs/issues/IF-116.md) | P2 | Open | Engine/composition | Persistence truncation corrupts live child-workflow results |
| [IF-117](docs/issues/IF-117.md) | P2 | Open | Composition/tool security | Missing nested tool-input paths fall back to the entire root object |
| [IF-118](docs/issues/IF-118.md) | P2 | Open | Composition/correctness | Child context overwrites authoritative parallel-result metadata |
| [IF-119](docs/issues/IF-119.md) | P2 | Open | Composition/correctness | Dynamic fan-out reinterprets literal string items as context keys |
| [IF-120](docs/issues/IF-120.md) | P2 | Open | Lua/validation | Validation rejects a valid callback reused on two steps |
| [IF-121](docs/issues/IF-121.md) | P2 | Open | Redis/atomicity | Redis lease-aware mutations partially commit before type errors |
| [IF-122](docs/issues/IF-122.md) | P2 | Open | SQL/concurrency | SQL's unowned context merge loses concurrent successful updates |
| [IF-123](docs/issues/IF-123.md) | P2 | Open | CLI/replica safety | Replica mode trusts backend labels instead of actual SQL dialects |
| [IF-124](docs/issues/IF-124.md) | P2 | Open | MCP/framing | MCP stdio drops partially consumed frames on select cancellation |
| [IF-125](docs/issues/IF-125.md) | P2 | Open | Schema/runtime safety | External JSON Schema references panic inside async node execution |
| [IF-126](docs/issues/IF-126.md) | P2 | Open | Cache/identity | File-cache sanitization aliases distinct logical keys |
| [IF-127](docs/issues/IF-127.md) | P2 | Open | ArangoDB/pagination | Arango cursor IDs are discarded |
| [IF-128](docs/issues/IF-128.md) | P2 | Open | AI/chunking | Semantic chunking selects minima of a distance signal |
| [IF-129](docs/issues/IF-129.md) | P2 | Open | Notification/resource safety | Notification response bodies bypass HTTP body admission limits |
| [IF-130](docs/issues/IF-130.md) | P2 | Open | ZIP/admission | ZIP duplicate handling and entry limits occur after deduplicating metadata |
| [IF-131](docs/issues/IF-131.md) | P2 | Open | S3/copy correctness | S3 copy source keys are not URL-encoded |
| [IF-132](docs/issues/IF-132.md) | P2 | Open | Artifact/cancellation | A continuously progressing artifact download delays cancellation until EOF |

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
