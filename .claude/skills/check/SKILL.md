---
name: check
description: Run focused or integration validation for this Rust project. Use when the user asks to check, validate, lint, test, or verify the repository state.
argument-hint: [focus|integration]
disable-model-invocation: true
user-invocable: true
allowed-tools: Bash
---

# Check

Choose the smallest gate that proves the requested change. Do not run tests or
Clippy automatically on every file save.

## Focused gate

This is the default for routine tasks. Inspect the changed paths and run only
the related unit or integration test targets and surface-specific validators.

For Rust changes, always finish with:

1. `cargo fmt --all -- --check`
2. `python3 -B scripts/check_module_size.py`
3. `cargo clippy --all-targets -- -D warnings`
4. focused test targets for the behavior changed

Add a feature check or disposable live-service test only when Redis/PostgreSQL
behavior requires it. For docs, Lua, hooks, skills, or workflows without Rust
changes, run only their relevant validators.

## Integration gate

Run `scripts/integration_gate.sh` only when:

- merging branches;
- immediately before pushing `develop`;
- preparing a release;
- the user explicitly requests the whole suite.

The script is authoritative for the complete local suite and uses disposable
Redis/PostgreSQL containers. Never substitute shared or production services.

Report every selected command as pass, fail, skipped, or not applicable, and
do not describe a skipped service test as a pass. Do not modify failures unless
the user also asked for fixes.
