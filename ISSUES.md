# IronFlow engineering issues

The canonical engineering ledger is maintained in
[`docs/issues/README.md`](docs/issues/README.md). Individual findings use
stable paths such as [`docs/issues/IF-001.md`](docs/issues/IF-001.md).

## Active findings

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
| [IF-133](docs/issues/IF-133.md) | P2 | Open | Documents/HTML extraction | `extract_html` markdown mode leaks head, style and script content |
| [IF-134](docs/issues/IF-134.md) | P3 | Open | Files/write_file | File-writing nodes disagree about symlinked destination roots |
| [IF-135](docs/issues/IF-135.md) | P2 | Open | Workflow composition | A failed sub-workflow reports only its status, not the failing task's error |
| [IF-136](docs/issues/IF-136.md) | P2 | Open | Lua runtime/conversion | Step handlers convert the entire run context under the JSON-to-Lua budget |
| [IF-137](docs/issues/IF-137.md) | P1 | Open | Storage/Postgres | Postgres and Redis stores cannot connect over TLS: sqlx has no TLS feature |
| [IF-138](docs/issues/IF-138.md) | P3 | Open | Database/ArangoDB | `arangodb_aql` bind variables from context are always strings |

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
