# IronFlow engineering issues

The canonical engineering ledger is maintained in
[`docs/issues/README.md`](docs/issues/README.md). Individual findings use
stable paths such as [`docs/issues/IF-001.md`](docs/issues/IF-001.md).

## Active findings

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
| [IF-115](docs/issues/IF-115.md) | P2 | Open | PPTX/page fidelity | PPTX extraction uses filenames instead of presentation order and membership |
| [IF-116](docs/issues/IF-116.md) | P2 | Open | Engine/composition | Persistence truncation corrupts live child-workflow results |
| [IF-118](docs/issues/IF-118.md) | P2 | Open | Composition/correctness | Child context overwrites authoritative parallel-result metadata |
| [IF-119](docs/issues/IF-119.md) | P2 | Open | Composition/correctness | Dynamic fan-out reinterprets literal string items as context keys |
| [IF-120](docs/issues/IF-120.md) | P2 | Open | Lua/validation | Validation rejects a valid callback reused on two steps |
| [IF-121](docs/issues/IF-121.md) | P2 | Open | Redis/atomicity | Redis lease-aware mutations partially commit before type errors |
| [IF-122](docs/issues/IF-122.md) | P2 | Open | SQL/concurrency | SQL's unowned context merge loses concurrent successful updates |
| [IF-123](docs/issues/IF-123.md) | P2 | Open | CLI/replica safety | Replica mode trusts backend labels instead of actual SQL dialects |
| [IF-124](docs/issues/IF-124.md) | P2 | Open | MCP/framing | MCP stdio drops partially consumed frames on select cancellation |
| [IF-127](docs/issues/IF-127.md) | P2 | Open | ArangoDB/pagination | Arango cursor IDs are discarded |
| [IF-128](docs/issues/IF-128.md) | P2 | Open | AI/chunking | Semantic chunking selects minima of a distance signal |

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
