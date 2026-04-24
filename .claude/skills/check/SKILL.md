---
name: check
description: Run the project's Rust validation workflow. Use when the user asks to check, validate, lint, test, or verify the repo state.
argument-hint: [optional-focus]
disable-model-invocation: true
user-invocable: true
allowed-tools: Bash
---

# Check

Run this workflow from the project root:

1. `cargo fmt --all --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test --all`

Rules:

- Execute the commands in that order.
- Stop immediately if `cargo fmt --all --check` or `cargo clippy --all-targets -- -D warnings` fails, and report the failing command plus the key error output.
- If `cargo test --all` fails, report the failing tests and the most relevant error output.
- Do not change files automatically as part of `/check` unless the user explicitly asks for fixes.
- Keep the final report concise: one line per command with `PASS` or `FAIL`, then the most important failure details if any.
- Ignore the optional argument unless the user clearly asks for a narrower check. If they do, explain that the repo standard check is still the default and only narrow the scope on explicit request.
