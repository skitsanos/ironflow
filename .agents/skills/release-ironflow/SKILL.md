---
name: release-ironflow
description: Prepare, verify, version, and publish an IronFlow release from verified develop through a finalization branch to main. Use when the user asks to release, cut a release, ship a version, bump the version for release, or integrate dependency updates into a release. Do not use for routine dependency updates, PR review alone, or deployment without release intent.
---

# Release IronFlow

Use explicit user gates before merges, version decisions, tags, pushes,
publishing, or closing remote PRs. Never force-push or delete a remote tag.

## 1. Establish release state

1. Require a clean worktree. Record the current branch, `Cargo.toml` version,
   latest `v*` tag, and whether local `develop` and `main` are current with the
   remote.
2. Confirm this repository still uses `develop` for integration and `main` for
   releases. The release tag must point to a commit reachable from `main`.
3. Stop if the manifest version and latest tag are inconsistent or if unrelated
   local changes would enter the release.
4. List every open PR targeting `develop` and summarize mergeability, review
   state, and CI. All must be merged, closed, or integrated and closed before a
   `develop` push. Ask before changing remote state.

## 2. Integrate dependency work

Consolidate compatible dependency changes on `develop` instead of blindly
merging Renovate PRs one by one. Run `cargo update`, inspect direct outdated
dependencies, and run `cargo audit`. Review stale audit ignores and
security-sensitive dependency behavior. Ask before accepting incompatible major
updates.

## 3. Verify the candidate

Run `scripts/integration_gate.sh`, which includes the complete
`$check-ironflow` workflow, all Lua examples, the release build, and disposable
live Redis/PostgreSQL suites. Every gate must pass. Do not proceed on skipped
required tests or by adding new audit/module-size exceptions without review.

## 4. Propose the version

Read commits and resolved `IF-NNN` entries since the latest tag. Propose a
Semantic Versioning bump based on behavior, not commit-message prefixes. Draft
release notes grouped by area and referencing the relevant issue IDs. Wait for
the user's version approval. Integration versions on `develop` use
`X.Y.Z-dev.N`; each push must be newer than the version currently on remote
`develop`. Use `bun run scripts/development_version.ts bump next` for another
push of the same candidate, or choose `major`, `minor`, or `patch` when starting
a candidate from a stable version.

## 5. Cut the release

After approval:

1. Create `release/X.Y.Z` from the verified `develop` commit. Never finalize on
   `develop`.
2. Run `bun run scripts/development_version.ts finalize` on the release branch
   to set stable `X.Y.Z` in both `Cargo.toml` and `Cargo.lock`, read both back,
   and rerun `scripts/integration_gate.sh`.
3. Commit and push the release branch only when the user authorized those actions.
4. Open `release/X.Y.Z` to `main`, wait for approval, and merge according to
   repository policy. The push to `main` runs CI.
5. Confirm the exact `main` CI run passed its Windows release-cache primer. A
   later cache miss does not invalidate the binaries, but it does leave release
   performance acceptance unproven.
6. Update local `main` with a fast-forward pull. Create an annotated `vX.Y.Z`
   tag only on the verified release commit.
7. Prove the tag is reachable from `main` and its `Cargo.toml` contains the same
   version. Show this evidence and ask before pushing the tag.
8. Push the tag, monitor the release workflow, and run publish steps only after
   separate user authorization. Confirm both Windows variants restored the
   shared default-branch dependency cache; each variant still compiles and
   packages its own binary from the tagged source.

If an unpushed local release step is wrong, use a new corrective edit or ask
before any destructive recovery. Once a tag is pushed, stop and request
direction rather than deleting or rewriting published history.
