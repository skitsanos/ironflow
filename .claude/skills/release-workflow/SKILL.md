---
name: rust-release-workflow
description: Use when the user asks to check PRs, update dependencies, run the test suite, and potentially cut a new release for this Rust project. Triggers on phrases like "release", "cut a release", "check PRs and release", "update deps and ship", "bump version". Do NOT use for routine dependency updates without release intent, or for PR review alone.
---

# Rust Release Workflow

A gated pipeline. Stop and ask the user before proceeding past any ❓ gate.

## Branch model

**This repo uses `develop` for active work and `main` for releases.**

- All feature, dependency, and chore commits land on `develop` (directly or via PR).
- Releases are cut from `main` only. The release tag (`vX.Y.Z`) MUST point to a commit reachable from `main`.
- The flow is: `develop` → PR to `main` → merge → tag on `main` → push tag → `release.yml` builds artifacts.
- `main` is protected: it requires a PR with approval. Direct push to `main` is not allowed and a CI `branch-guard` job rejects PRs to `main` from any branch other than `develop`.

Throughout this skill, "default branch" means **`main` for tagging** and **`develop` for accepting incoming work**. The two are not interchangeable.

## Stage 0 — Version Sync Check

Before doing anything else, verify the repo is in a clean release state:

1. Read version from `Cargo.toml` → call it `CARGO_VERSION`.
2. Get latest tag: `git describe --tags --abbrev=0` → call it `LAST_TAG` (strip leading `v`).
3. Check working tree clean: `git status --porcelain` must be empty.
4. Note the current branch with `git rev-parse --abbrev-ref HEAD`. For this repo, expect `develop` while preparing the release; you'll switch to `main` only at Stage 5.
5. Check up to date with remote: `git fetch && git status -sb`. Both `develop` and `main` should be up to date with their remote counterparts before continuing.

Compare `CARGO_VERSION` and `LAST_TAG`:

- **Equal** → normal state. Proceed.
- **`CARGO_VERSION` > `LAST_TAG`** → previous release was bumped but never tagged. ❓ **Gate:** offer to tag the next release as `v$CARGO_VERSION` (after the develop→main merge), OR investigate first.
- **`LAST_TAG` > `CARGO_VERSION`** → tag exists ahead of manifest. This shouldn't happen in a healthy repo. STOP and report — do not auto-fix.
- **Working tree dirty / behind remote** → STOP. Release from a clean, current state only.

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

The bump can land on `develop` first (recommended — the develop→main PR carries it) or directly on `main` after the merge. Either way, **the tag is created from `main` after the develop→main merge has landed.**

1. **On `develop`:** edit `version` in `Cargo.toml`. For workspaces, update every member that ships (or the workspace `[workspace.package].version` if inherited).
2. **Refresh lockfile:** `cargo build` (or `cargo update -p <this-crate>` for a minimal lock change). `Cargo.lock` MUST be committed.
3. **Update CHANGELOG.md** with the entry generated from commits since `LAST_TAG`.
4. **Verify before committing:** `grep '^version' Cargo.toml` — confirm the new version is actually written. Don't trust the edit succeeded; read it back.
5. **Commit on `develop`:** `git commit -am "chore: release vX.Y.Z"`. The commit message MUST contain the exact version string for grep-ability later. Push with `git push origin develop`.
6. **Open `develop` → `main` PR:** title `Release vX.Y.Z`, body lists the changes. Wait for CI green and approval; merge with a merge commit (no squash — preserve history on `main`).
7. **Switch to `main` and pull:** `git checkout main && git pull --ff-only origin main`. Verify `git log -1 --format='%s'` shows the merge / release commit.
8. **Tag on `main`:** `git tag -a vX.Y.Z -m "Release vX.Y.Z"`. Do NOT use lightweight tags — annotated tags carry metadata and are what `git describe` prefers.
9. **Verify tag points to a commit on `main`:** `git branch --contains vX.Y.Z` must list `main`.
10. **Verify tag matches manifest:** `git show vX.Y.Z:Cargo.toml | grep '^version'` must equal `X.Y.Z`. This catches the case where you tagged the wrong commit.
11. Show the user: the bump diff, the tag, the commit it points to. ❓ **Gate:** confirm before pushing.
12. **Push the tag:** `git push origin vX.Y.Z`. The `release.yml` workflow has a `guard` job that verifies the tag commit is reachable from `main` before building; if you tagged the wrong branch, the release will fail in CI.
13. If publishing to crates.io: `cargo publish --dry-run`, then on user confirmation `cargo publish`. Do this AFTER the tag is pushed so the published crate is reproducible from the tag.

### Recovery

If anything fails before the tag is pushed (steps 1–11):
- Local-only tag: `git tag -d vX.Y.Z` removes it.
- If the bump commit is on `develop` but you want to re-do it: `git reset --hard HEAD~1` (only if not yet pushed) or open a fix-up commit (if already pushed). Never force-push `develop` if other contributors are working from it.

If the tag has been pushed and is wrong: STOP and ask the user. Never delete remote tags automatically — downstream consumers may already have pulled them.

## Notes

- Never `git push --force`. Never amend an already-pushed tag.
- If the repo uses `release-plz` or `cargo-release`, prefer those tools over manual steps — check for their config files first (`release-plz.toml`, `release.toml`).
- For workspaces, confirm whether all member crates bump together or independently before Stage 5.