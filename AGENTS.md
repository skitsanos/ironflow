# IronFlow repository guidance

## Scope and source of truth

- IronFlow is a Rust workflow engine whose workflow definitions and examples are Lua.
- Treat `src/`, `docs/`, `examples/`, tests, and the public CLI/API contracts as one product surface.
- Use `docs/issues/README.md` as the engineering registry and `docs/issues/IF-NNN.md` as the canonical finding record. Root `ISSUES.md` is a generated compatibility index. An entry is not resolved until its implementation, regressions, documentation, and required validation are complete.
- Preserve user-owned and unrelated worktree changes. Inspect `git status` and the relevant diff before editing overlapping files.
- Work in English. Do not commit, push, tag, publish, merge, or alter releases unless the user explicitly asks.

## Rust design rules

- Keep production modules focused and normally at or below 300 lines. The hard ceiling is 400 lines. Test-only files (`tests.rs`, `*_tests.rs`, `test_support.rs`, and `tests/` or `test_support/` directories) are outside the gate.
- Keep functions at or below 100 lines and cognitive complexity 25 (`clippy.toml`). Refactor first; a scoped `#[allow]` must name the single responsibility that justifies the length, and no change may raise a threshold.
- Run `python3 -B scripts/check_module_size.py` after Rust changes. Do not add or widen a module-size exception merely to make the gate pass; split responsibilities first. When a reviewed module shrinks, run the checker with `--ratchet` to record the lower ceiling.
- Prefer small modules, typed errors and state transitions, explicit ownership, and bounded work over large coordinating functions or implicit side effects.
- Keep async runtime threads free of blocking filesystem, archive, parsing, database-driver, or process work. Use the existing blocking/cancellation bridges and retain admission permits until physical work stops.
- Treat Lua, paths, URLs, archives, provider responses, storage records, and environment configuration as untrusted input. Enforce limits before materialization and keep secrets out of errors and logs.
- Avoid new dependencies when the standard library or an existing dependency is sufficient. Do not weaken `.cargo/audit.toml` without documenting and reviewing the advisory.
- The JSON state store (`src/storage/json_store`) is maintenance-only since IF-163: fix defects and keep the `StateStore` contract, but add no features. SQLite is the default backend; new storage behavior belongs to the SQL and Redis stores.

## Documentation and examples

- When behavior changes, update the nearest node document plus any affected CLI, architecture, README, implementation-plan, and example surfaces.
- Keep node names, parameters, defaults, environment variables, status codes, limits, and failure behavior consistent across code and documentation.
- Add or update Lua examples for public behavior. Keep `examples/catalog.json`, `examples/README.md`, fixtures, and checksums consistent when applicable.
- Classify old plans and investigation notes honestly; do not present historical evidence as the current runtime contract.
- After changing issue metadata or records, run `bun run scripts/issues_registry.ts generate` and then `bun run scripts/issues_registry.ts check`.

## Repository tooling

- `.agents/skills/` holds the canonical repository skills (`check-ironflow`, `release-ironflow`, `resolve-ironflow-issue`). `.claude/skills/check` and `.claude/skills/release-workflow` are thin shims that defer to them: edit the canonical skill, never the shim. `bun run scripts/validate_skills.ts` validates both and rejects a shim that names a missing skill, exceeds 15 non-empty lines, or restates sections. `.codex/` holds only the Codex hooks (`protect_paths.py`, `session_context.py`) and their manifest.
- Every script has one of four reasons to exist: gates (`check_module_size.py`, `check_licenses.ts`, `validate_skills.ts`, `issues_registry.ts`, `development_version.ts`, `check_incoming_prs.ts`, `integration_gate.sh`, `gate_receipt.ts`), the bounded test runner (`test_rust_targets.ts`), live-service acceptance (`test_store_tls.ts` with `storage_tls/`, `test_container_store.ts`, `test_arango_typed_binds.ts`, `test_public_tls.ts`, `replica_acceptance.ts`), and release automation (`tag_container_release.ts`). `extraction_benchmark.ts` is a developer benchmark, not a gate.
- `scripts/tests/` is the single policy test entry: `python3 -B -m unittest discover -s scripts/tests -p 'test_*.py'` covers the Codex hooks, the pre-commit hook and the module-size checker; `bun test scripts/tests/*.test.ts` covers the scripts and repository policy. A policy test must protect behavior (version arithmetic, registry parsing, cleanup traps, target selection, supply-chain pins); do not add tests that only restate YAML, TOML or shell text. The keep/merge/delete inventory is recorded in `docs/issues/IF-169.md`.

## Validation

- Do not run tests or Clippy merely because a file was saved. During ordinary work, run only the tests and validators relevant to the behavior changed, including negative and cancellation cases where appropriate.
- Every Rust task still finishes with `cargo fmt --all -- --check`, `python3 -B scripts/check_module_size.py`, and the exact required lint command: `cargo clippy --all-targets -- -D warnings`. Add only focused test targets for the changed code.
- Integration tests build as area binaries declared in `Cargo.toml`: each `tests/<area>/main.rs` includes unchanged `tests/test_*.rs` files as modules and test autodiscovery is off. Select one area with `cargo test --test <area>` and one file with `cargo test --test <area> <module>::`. Put a new test file in the matching area root; a file that mutates process environment variables goes in `tests/serial/main.rs`, which must run with `-- --test-threads=1`; only a test that configures a process-wide `OnceLock` from the environment before anything else reads it gets its own `[[test]]` entry. `scripts/tests/test_layout.test.ts` fails when a file is left out.
- For `postgres` or `redis` work, add the relevant feature check or focused feature test. Use live storage only when the changed behavior requires it.
- For docs, examples, hooks, skills, or workflows without Rust changes, run only their relevant validators. Run `cargo test --doc` after public Rust API documentation changes, and `cargo audit --deny warnings` plus `bun --no-env-file scripts/check_licenses.ts` after dependency or security-sensitive changes.
- Validate affected Lua examples after docs/example-only changes. Validate every Lua example only after node registration, Lua runtime, or broad public workflow changes, or at the integration boundary.
- Run `actionlint .github/workflows/*.yml` after workflow changes when `actionlint` is available.
- Run `bun run scripts/validate_skills.ts` after changing repository skills or their Claude shims. Repository-owned YAML validation must use Bun and must not add a Python YAML dependency.
- Storage integration tests are destructive. Use disposable, explicitly named Redis/PostgreSQL instances with required-test flags; never use a shared or production service and remove only the containers created for the test.
- Never expose values from `.env`. Treat `.env`, `.env.*` other than `.env.example`, private keys, `secrets/`, and `.git/` as protected paths.

## Integration and release boundaries

- Run the whole repository suite only when merging branches, immediately before pushing `develop`, preparing a release, or when the user explicitly requests it. Use `scripts/integration_gate.sh` for the local full gate.
- Track the newest stable Rust release with reproducible exact pins. A toolchain update is incomplete until `rust-toolchain.toml`, every CI/release `dtolnay/rust-toolchain` reference, the cargo-chef Docker tag and digest, and policy expectations agree; do not replace these with floating `stable` or `nightly` release-build inputs.
- The integration gate disables Cargo incremental output and package-cleans only IronFlow workspace artifacts before and after the run. It preserves downloaded dependencies but intentionally removes local IronFlow binaries and linked test executables so repeated versioned gates cannot accumulate tens of gigabytes.
- Within each local/CI test phase, `scripts/test_rust_targets.ts` runs every Cargo test target from metadata: the parallel-safe targets in one Cargo invocation, then `tests/serial` with `--test-threads=1`. It preserves feature/harness flags and reclaims only Cargo-reported test executables afterwards; normal CLI binaries, libraries and dependency caches remain available. `--test <name>` (repeatable) narrows a run to named area binaries: CI's feature job runs only the binaries with feature-gated tests, and macOS pull-request runs cover `nodes`, `cli`, `engine` and `serial`, with the full suite on pushes. Without `--test` this is full-target coverage, not a focused-test shortcut.
- Before every push to `develop`, inspect all open pull requests targeting `develop`. Merge them, close them, or integrate their work and close them before proceeding. The pre-push hook fails closed when any remain open.
- `develop` versions use `X.Y.Z-dev.N`. Before every push to `develop`, bump to a version newer than remote `develop` with `bun run scripts/development_version.ts bump <major|minor|patch|next>` and commit both `Cargo.toml` and `Cargo.lock`.
- A `develop` push requires a clean worktree and a passing `scripts/integration_gate.sh` run on the exact tree being pushed. The gate cannot run inside the push itself, because GitHub closes the idle SSH session before it finishes (IF-172); instead a passing gate records `ironflow-gate-receipt.json` in the Cargo target directory, and `.githooks/pre-push` accepts only a receipt that is at most 24 hours old and whose tree matches the pushed commit (`scripts/gate_receipt.ts`). `IRONFLOW_PREPUSH_GATE=fast` replaces the receipt check with formatting, repository policy, default Clippy and the `examples` and `cli` binaries for an intermediate push, and the full gate remains mandatory before merging a release branch or tagging. CI runs for relevant pushes and pull requests to `develop` and `main`, plus manual dispatch; the tag-triggered release workflow remains separate.
- For release promotion, create `release/X.Y.Z` from the verified `develop` candidate, run `bun run scripts/development_version.ts finalize` on that release branch, run the integration gate, merge the release branch into `main`, require green `main` CI, and tag the verified `main` commit. Never put a stable version on `develop`.

## Completion and review

- Update the relevant `docs/issues/IF-NNN.md` page with the implemented outcome, contract boundary, and concrete validation evidence before marking it resolved, then regenerate the indexes.
- Distinguish focused, default, feature-enabled, live-service, and deployed validation. Do not claim one as evidence for another.
- After completing a goal or task, propose no more than three prioritized, bounded development goals.
- In code review, prioritize correctness, security, durability, cancellation, bounded resource use, docs/example parity, and missing regressions over style-only comments.
- Preserve each review finding's original trigger and evidence boundary in its canonical issue page. Distinguish pre-fix baseline gates from post-fix regression results; a prior green suite never closes a newly demonstrated defect.
- Repository Codex hooks are guardrails, not a filesystem sandbox. Test supported hook payloads and protected path aliases; do not claim shell/read coverage or actual app invocation from hook-script tests alone.
