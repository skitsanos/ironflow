---
name: check
description: Run the project's Rust validation workflow. Use when the user asks to check, validate, lint, test, or verify the repo state.
argument-hint: [optional-focus]
disable-model-invocation: true
user-invocable: true
allowed-tools: Bash
---

# Check

Run the same gates CI runs, so a green `/check` means a green pipeline. If this
list and CI ever diverge, CI is the source of truth — reconcile this file
against `.github/workflows/ci.yml`.

Run from the project root, in this order (cheapest first, so failures surface
fast):

1. `cargo fmt --all -- --check`
2. `python3 -B scripts/check_module_size.py`
3. `python3 -B -m unittest discover -s scripts/tests -p 'test_*.py'`
4. `cargo clippy --all-targets -- -D warnings`
5. `cargo check --all-targets --features postgres,redis`
6. `cargo test --all-targets`
7. `cargo audit`

Rules:

- Execute in that order. Steps 1–3 take seconds; do not skip them to "save
  time" by jumping straight to the tests.
- Stop immediately if steps 1–5 fail. Report the failing command and the key
  error output.
- If step 6 fails, report the failing test names and the most relevant output,
  then still run step 7 and report it.
- Do not change files as part of `/check` unless the user explicitly asks for
  fixes.
- Final report: one line per command with `PASS` or `FAIL`, then failure detail
  only for what failed.
- Ignore the optional argument unless the user clearly asks for a narrower
  check. If they do, say the full check is still the repo standard and only
  narrow on explicit request.

## Why each step is here

Steps 2 and 3 enforce the module-size ratchet (`scripts/module_size_policy.json`).
Its reviewed-exception budget is capped, so a file crossing 300 lines fails the
build and cannot be waved through by adding an exception — it usually means a
module has taken on a second responsibility and wants splitting. This gate has
blocked real work; leaving it out of `/check` just moves the failure to CI.

Step 5 exists because `postgres` and `redis` are feature-gated. Code that
compiles under default features can fail under those, and CI checks them
separately.

Step 7 fails on any advisory not listed in `.cargo/audit.toml`. That ignore list
is deliberately small and every entry is transitive; when `cargo audit` fails,
the fix is normally a dependency bump, not a new ignore entry.

## Not covered here

Too slow for an interactive check. Run explicitly when the change warrants it:

- Release build plus example validation — run after touching node registration,
  the Lua API, or any example:
  ```
  cargo build --release
  find examples -type f -name '*.lua' -print0 \
    | xargs -0 -n1 ./target/release/ironflow validate
  ```
- Redis integration tests, which need a disposable server:
  ```
  IRONFLOW_REDIS_TEST_URL=redis://127.0.0.1:6379 IRONFLOW_REDIS_TEST_REQUIRED=1 \
    cargo test --features redis --test test_redis_atomicity -- --test-threads=1
  ```
  Never point these at a shared or production Redis — they use `CLIENT PAUSE`,
  which is server-wide.

One caution: `.env` may define `DATABASE_URL`. If it points at a real database,
the Postgres integration tests will run against it, and they are destructive
(concurrent init, delete, prune). Unset it before running the suite unless you
intend that.
