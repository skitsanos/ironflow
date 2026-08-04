---
name: check-ironflow
description: Run IronFlow's Rust, policy, documentation, Lua-example, feature, audit, and optional live-storage validation. Use when the user asks to check, validate, lint, test, verify, audit, or confirm that an IronFlow change is ready. Do not modify failures unless the user also asks for fixes.
---

# Check IronFlow

Run from the repository root. Treat `.github/workflows/ci.yml` as authoritative
if it has diverged from this workflow, then reconcile this skill as part of the
task.

## Select the gate

- Use the focused gate for ordinary tasks: relevant unit/integration tests plus
  checks for the surfaces changed. Do not run tests merely because a file was saved.
- For Rust changes, finish with `cargo fmt --all -- --check`, the module-size
  policy, `cargo clippy --all-targets -- -D warnings`, and focused tests.
- Use the full gate only for branch integration, immediately before a
  `develop` push, release preparation, or an explicit user request. Prefer
  `scripts/integration_gate.sh` so local integration matches repository policy.
- Add example validation when Lua runtime, node registration, docs, or examples
  change.
- Run `bun run scripts/issues_registry.ts check` when issue pages, issue
  indexes, repository guidance, or issue-resolution skills change.
- Add live storage validation when Redis/PostgreSQL behavior, schemas, leases,
  claims, retention, or event stores change.

Never point integration tests at shared or production infrastructure.

## Run the full integration gate

`scripts/integration_gate.sh` is authoritative. It runs inexpensive failures
first, then the following Rust and live-service gates:

The local full gate sets `CARGO_INCREMENTAL=0` and runs package-scoped
`cargo clean --package ironflow` before and after validation. Dependency caches
remain available, but IronFlow binaries and linked test executables are
intentionally removed so repeated versioned gates do not exhaust local disk.

1. `cargo fmt --all -- --check`
2. `git diff --check`
3. `python3 -B -m unittest discover -s scripts/tests -p 'test_*.py' -v`
4. `python3 -B scripts/check_module_size.py`
5. `bun run scripts/validate_skills.ts`
6. `actionlint .github/workflows/*.yml` when available and workflows changed
7. `cargo check --all-targets`
8. `cargo clippy --all-targets -- -D warnings`
9. `cargo test --all-targets`
10. `cargo test --doc`
11. `cargo check --all-targets --features postgres,redis`
12. `cargo clippy --all-targets --features postgres,redis -- -D warnings`
13. `cargo audit --deny warnings`

Stop after a failure and report the actionable output. If tests fail, identify
the failing test and preserve enough output to diagnose it.

## Validate Lua examples

Build once, then validate every discovered example without assuming a fixed
count:

```bash
cargo build
failures=0
count=0
while IFS= read -r -d '' flow; do
  count=$((count + 1))
  if ! ./target/debug/ironflow validate "$flow"; then
    failures=$((failures + 1))
  fi
done < <(find examples -type f -name '*.lua' -print0)
test "$failures" -eq 0
printf 'validated %s Lua examples\n' "$count"
```

Use a Bash-compatible shell for this loop. Do not use a release build merely to
duplicate already compiled artifacts when disk pressure makes that unsafe; say
which binary was used.

## Validate live stores

Start disposable, explicitly named Redis and PostgreSQL containers. Wait for
their health checks, set `IRONFLOW_REDIS_TEST_REQUIRED=1` and
`IRONFLOW_POSTGRES_TEST_REQUIRED=1`, then run the relevant feature suites
serially. Record image/database versions and remove only those named containers
afterward. Never read or print credentials from `.env`.

## Report

Report each command as pass, fail, skipped, or not applicable. Separate focused,
default, feature-enabled, live-service, and deployed evidence. Do not describe a
skipped service test as a pass.
