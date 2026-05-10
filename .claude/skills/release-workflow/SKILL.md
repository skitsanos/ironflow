---
name: rust-release-workflow
description: Use when the user asks to check PRs, update dependencies, run the test suite, and potentially cut a new release for this Rust project. Triggers on phrases like "release", "cut a release", "check PRs and release", "update deps and ship", "bump version". Do NOT use for routine dependency updates without release intent, or for PR review alone.
---

# Rust Release Workflow

A gated pipeline. Stop and ask the user before proceeding past any ❓ gate.

## Stage 0 — Version Sync Check

Before doing anything else, verify the repo is in a clean release state:

1. Read version from `Cargo.toml` → call it `CARGO_VERSION`.
2. Get latest tag: `git describe --tags --abbrev=0` → call it `LAST_TAG` (strip leading `v`).
3. Check working tree clean: `git status --porcelain` must be empty.
4. Check on default branch: `git rev-parse --abbrev-ref HEAD` should be `main` (or whatever the repo uses).
5. Check up to date with remote: `git fetch && git status -sb`.

Compare `CARGO_VERSION` and `LAST_TAG`:

- **Equal** → normal state. Proceed.
- **`CARGO_VERSION` > `LAST_TAG`** → previous release was bumped but never tagged. ❓ **Gate:** offer to tag `HEAD` as `v$CARGO_VERSION` and push, OR investigate first.
- **`LAST_TAG` > `CARGO_VERSION`** → tag exists ahead of manifest. This shouldn't happen in a healthy repo. STOP and report — do not auto-fix.
- **Working tree dirty / not on default branch / behind remote** → STOP. Release from a clean, current default branch only.

For workspaces: check every member crate's version, not just the root. All must agree (unless the project intentionally versions members independently — check for that pattern in prior tags like `crate-name-vX.Y.Z`).

## Stage 1 — Incoming PRs

Run `gh pr list --state open --json number,title,author,mergeable,reviewDecision,statusCheckRollup`.

For each PR, summarize: number, title, mergeable state, review decision, CI status.

❓ **Gate:** Present the list to the user. Ask which (if any) to merge before proceeding. If none ready, ask whether to continue with the release anyway or stop.

## Stage 2 — Dependency Updates

Run in this order, stopping on first failure:

1. `cargo update` — semver-compatible lockfile updates only.
2. `cargo outdated --depth 1` — show what's pinned behind major versions. Report but do NOT auto-bump these; major bumps are a Stage-2.5 conversation.
3. If `cargo-audit` is installed: `cargo audit` — flag advisories.

❓ **Gate:** If `cargo outdated` shows major-version drift OR `cargo audit` reports anything, surface it and ask whether to address now or defer.

## Stage 3 — Verify

Run all of these. ALL must pass:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo build --release`
- `cargo test --all-features`

If any fail: stop, report, do not proceed to release. Offer to fix formatting/clippy automatically; do not auto-fix test failures.

## Stage 4 — Version Decision

1. Get last tag: `git describe --tags --abbrev=0`.
2. Get commits since: `git log <last-tag>..HEAD --oneline`.
3. Classify per Conventional Commits (or this project's convention — see CHANGELOG.md for prior style):
    - `feat!:` / `BREAKING CHANGE` → major
    - `feat:` → minor
    - `fix:`, `perf:`, `refactor:` → patch
    - docs/chore/test only → suggest skipping the release
4. Read current version from `Cargo.toml`.

❓ **Gate:** Propose the new version with reasoning ("12 commits since v0.4.2: 3 feat, 5 fix → v0.5.0"). Wait for user confirmation before bumping.

## Stage 5 — Cut Release (atomic bump-commit-tag)

Only after user confirms the new version. The order matters — do not reorder.

1. **Bump manifest(s):** edit `version` in `Cargo.toml`. For workspaces, update every member that ships (or the workspace `[workspace.package].version` if inherited).
2. **Refresh lockfile:** `cargo build` (or `cargo update -p <this-crate>` if you want a minimal lock change). `Cargo.lock` MUST be committed.
3. **Update CHANGELOG.md** with the entry generated from commits since `LAST_TAG`.
4. **Verify before committing:** `grep '^version' Cargo.toml` — confirm the new version is actually written. Don't trust the edit succeeded; read it back.
5. **Commit:** `git commit -am "chore: release vX.Y.Z"`. The commit message MUST contain the exact version string for grep-ability later.
6. **Tag the commit just made:** `git tag -a vX.Y.Z -m "Release vX.Y.Z"`. Do NOT use lightweight tags — annotated tags carry metadata and are what `git describe` prefers.
7. **Verify tag points to bump commit:** `git rev-parse vX.Y.Z` and `git rev-parse HEAD` must match. If not, STOP — something went wrong.
8. **Verify tag matches manifest:** `git show vX.Y.Z:Cargo.toml | grep '^version'` must equal `X.Y.Z`. This catches the case where you tagged the wrong commit.
9. Show the user: the bump diff, the tag, the commit it points to. ❓ **Gate:** confirm before pushing.
10. **Push atomically:** `git push --atomic origin <branch> vX.Y.Z`. The `--atomic` flag ensures both the branch and tag arrive together — no half-pushed state where the tag exists on the server but the bump commit doesn't, or vice versa.
11. If publishing to crates.io: `cargo publish --dry-run`, then on user confirmation `cargo publish`. Do this AFTER the tag is pushed so the published crate is reproducible from the tag.

### Recovery

If anything fails between steps 5 and 10 and the tag is local-only:
- `git tag -d vX.Y.Z` removes the local tag.
- `git reset --hard HEAD~1` undoes the bump commit.
- Restart Stage 5.

If the tag has been pushed and is wrong: STOP and ask the user. Never delete remote tags automatically — downstream consumers may already have pulled them.

## Notes

- Never `git push --force`. Never amend an already-pushed tag.
- If the repo uses `release-plz` or `cargo-release`, prefer those tools over manual steps — check for their config files first (`release-plz.toml`, `release.toml`).
- For workspaces, confirm whether all member crates bump together or independently before Stage 5.