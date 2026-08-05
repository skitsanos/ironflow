# IronFlow engineering issues

The canonical engineering ledger is maintained in
[`docs/issues/README.md`](docs/issues/README.md). Individual findings use
stable paths such as [`docs/issues/IF-001.md`](docs/issues/IF-001.md).

## Active findings

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
| [IF-100](docs/issues/IF-100.md) | P1 | Open | Observability | Production deployments lack a bounded metrics contract |

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
