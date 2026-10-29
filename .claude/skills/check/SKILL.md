---
name: check
description: Run focused or integration validation for this Rust project. Use when the user asks to check, validate, lint, test, or verify the repository state.
argument-hint: [focus|integration]
disable-model-invocation: true
user-invocable: true
allowed-tools: Bash
---

Read and follow the canonical skill in `.agents/skills/check-ironflow/SKILL.md`.
This entry only maps `/check` onto it: `$ARGUMENTS` of `focus` (the default)
selects the focused gate for the changed surfaces, and `integration` runs
`scripts/integration_gate.sh`. Report each command as pass, fail, skipped, or
not applicable, and do not fix failures unless the user also asked for fixes.
