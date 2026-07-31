# IronFlow repository guidance

## Scope and source of truth

- IronFlow is a Rust workflow engine whose workflow definitions and examples are Lua.
- Treat `src/`, `docs/`, `examples/`, tests, and the public CLI/API contracts as one product surface.
- Use root `ISSUES.md` as the engineering ledger. An `IF-NNN` entry is not resolved until its implementation, regressions, documentation, and required validation are complete.
- Preserve user-owned and unrelated worktree changes. Inspect `git status` and the relevant diff before editing overlapping files.
- Work in English. Do not commit, push, tag, publish, merge, or alter releases unless the user explicitly asks.

## Rust design rules

- Keep production modules focused and normally at or below 300 lines. The hard ceiling is 400 lines.
- Run `python3 -B scripts/check_module_size.py` after Rust changes. Do not add or widen a module-size exception merely to make the gate pass; split responsibilities first.
- Prefer small modules, typed errors and state transitions, explicit ownership, and bounded work over large coordinating functions or implicit side effects.
- Keep async runtime threads free of blocking filesystem, archive, parsing, database-driver, or process work. Use the existing blocking/cancellation bridges and retain admission permits until physical work stops.
- Treat Lua, paths, URLs, archives, provider responses, storage records, and environment configuration as untrusted input. Enforce limits before materialization and keep secrets out of errors and logs.
- Avoid new dependencies when the standard library or an existing dependency is sufficient. Do not weaken `.cargo/audit.toml` without documenting and reviewing the advisory.

## Documentation and examples

- When behavior changes, update the nearest node document plus any affected CLI, architecture, README, implementation-plan, and example surfaces.
- Keep node names, parameters, defaults, environment variables, status codes, limits, and failure behavior consistent across code and documentation.
- Add or update Lua examples for public behavior. Keep `examples/catalog.json`, `examples/README.md`, fixtures, and checksums consistent when applicable.
- Classify old plans and investigation notes honestly; do not present historical evidence as the current runtime contract.

## Validation

- Do not run tests or Clippy merely because a file was saved. During ordinary work, run only the tests and validators relevant to the behavior changed, including negative and cancellation cases where appropriate.
- Every Rust task still finishes with `cargo fmt --all -- --check`, `python3 -B scripts/check_module_size.py`, and the exact required lint command: `cargo clippy --all-targets -- -D warnings`. Add only focused test targets for the changed code.
- For `postgres` or `redis` work, add the relevant feature check or focused feature test. Use live storage only when the changed behavior requires it.
- For docs, examples, hooks, skills, or workflows without Rust changes, run only their relevant validators. Run `cargo test --doc` after public Rust API documentation changes and `cargo audit` after dependency or security-sensitive changes.
- Validate affected Lua examples after docs/example-only changes. Validate every Lua example only after node registration, Lua runtime, or broad public workflow changes, or at the integration boundary.
- Run `actionlint .github/workflows/*.yml` after workflow changes when `actionlint` is available.
- Run `bun run scripts/validate_skills.ts` after changing repository skills. Repository-owned YAML validation must use Bun and must not add a Python YAML dependency.
- Storage integration tests are destructive. Use disposable, explicitly named Redis/PostgreSQL instances with required-test flags; never use a shared or production service and remove only the containers created for the test.
- Never expose values from `.env`. Treat `.env`, `.env.*` other than `.env.example`, private keys, `secrets/`, and `.git/` as protected paths.

## Integration and release boundaries

- Run the whole repository suite only when merging branches, immediately before pushing `develop`, preparing a release, or when the user explicitly requests it. Use `scripts/integration_gate.sh` for the local full gate.
- Before every push to `develop`, inspect all open pull requests targeting `develop`. Merge them, close them, or integrate their work and close them before proceeding. The pre-push hook fails closed when any remain open.
- `develop` versions use `X.Y.Z-dev.N`. Before every push to `develop`, bump to a version newer than remote `develop` with `bun run scripts/development_version.ts bump <major|minor|patch|next>` and commit both `Cargo.toml` and `Cargo.lock`.
- A `develop` push requires a clean worktree and runs the full integration gate through `.githooks/pre-push`. CI runs its full suite only for pushes to `develop` and `main`; the tag-triggered release workflow remains separate.
- For release promotion, create `release/X.Y.Z` from the verified `develop` candidate, run `bun run scripts/development_version.ts finalize` on that release branch, run the integration gate, merge the release branch into `main`, require green `main` CI, and tag the verified `main` commit. Never put a stable version on `develop`.

## Completion and review

- Update the relevant `ISSUES.md` entry with the implemented outcome, contract boundary, and concrete validation evidence before marking it resolved.
- Distinguish focused, default, feature-enabled, live-service, and deployed validation. Do not claim one as evidence for another.
- After completing a goal or task, propose no more than three prioritized, bounded development goals.
- In code review, prioritize correctness, security, durability, cancellation, bounded resource use, docs/example parity, and missing regressions over style-only comments.
