---
name: resolve-ironflow-issue
description: Implement one tracked IronFlow `IF-NNN` finding end to end across Rust code, tests, documentation, and Lua examples. Use when the user says to proceed with, fix, resolve, continue, or take the next IronFlow issue or development goal. Do not use for an untracked unrelated change unless the user asks to add it to the issue ledger.
---

# Resolve an IronFlow issue

## Establish the contract

1. Inspect `git status`, the current branch, and overlapping diffs. Preserve all
   unrelated changes.
2. Read the `IF-NNN` summary in `docs/issues/README.md` and the canonical
   `docs/issues/IF-NNN.md` page. Confirm its current status, required outcome,
   and boundaries against the live code.
3. Trace every affected entry point, storage backend, docs page, and example.
   Treat the ledger as a hypothesis until the current source confirms it.
4. State the implementation boundary when behavior depends on deployments,
   external providers, custom stores, or live services.

## Implement

- Write a focused regression that fails for the reported defect or missing
  behavior whenever practical.
- Fix the shared abstraction rather than patching only one caller. Cover all
  built-in backends or entry points governed by the contract.
- Keep new production modules at or below 300 lines and never above 400. Split
  orchestration, parsing, persistence, and policy into separate responsibilities.
- Preserve cancellation, admission ownership, bounded resource use, typed
  failures, secret redaction, and atomic storage semantics.
- Update public docs and runnable Lua examples in the same change. Keep defaults,
  parameters, limits, environment variables, and error behavior identical.

Do not broaden the issue into unrelated cleanup. Record newly confirmed,
independent defects as separate ledger candidates.

## Validate

1. Run focused tests while iterating, including negative, concurrency,
   cancellation, and boundary cases appropriate to the issue.
2. For Rust changes, run formatting, the module-size policy, exact all-target
   Clippy, and the focused tests selected for the issue. Do not run the whole
   suite for routine issue completion.
3. Use disposable Redis/PostgreSQL instances with required-test flags when the
   issue touches those backends. A skipped integration test is not evidence.
4. Validate every Lua example when the runtime, registry, docs, or examples
   change.

The full `$check-ironflow` integration gate is deferred to a branch merge, the
pre-push boundary for `develop`, release preparation, or an explicit user request.

## Close the ledger

Only after the required gates pass:

- mark the summary row and detailed entry resolved with the date;
- document the cause, implementation, focused coverage, contract boundary, and
  exact validation evidence;
- regenerate the root and documentation indexes with
  `bun run scripts/issues_registry.ts generate`, then run the matching `check`;
- run `git diff --check` and review the complete issue-scoped diff;
- report files changed and any remaining risks without claiming a commit,
  deployment, or live verification that did not occur;
- propose no more than three prioritized, bounded next development goals.
