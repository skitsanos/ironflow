---
name: release-workflow
description: Prepare and cut an IronFlow release through develop and main, including PR resolution, integration validation, versioning, and tagging.
---

# IronFlow release workflow

Use explicit user approval before merging, closing remote PRs, committing,
pushing, tagging, or publishing. Never force-push or delete a remote tag.

## Branch and version model

- `develop` is the integration branch and uses `X.Y.Z-dev.N`.
- Every push to `develop` must use a version newer than remote `develop` in both
  `Cargo.toml` and `Cargo.lock`.
- `main` is the release branch and uses stable `X.Y.Z`.
- Release tags are annotated `vX.Y.Z` tags on commits reachable from `main`.

Use `bun run scripts/development_version.ts bump <major|minor|patch|next>` to
prepare a development version. `next` advances an existing candidate.

## Prepare develop

1. Require a clean worktree and current remote state. Record branch, manifest
   version, latest tag, and divergence from `origin/develop` and `origin/main`.
2. List every open PR targeting `develop` with mergeability, review, and CI.
   Before pushing, merge it, close it, or integrate its work and close it.
   Request approval before any remote mutation.
3. Review changes since the latest tag and propose a SemVer candidate based on
   actual behavior and resolved `IF-NNN` entries.
4. Set a new `X.Y.Z-dev.N`, commit both version files, and run
   `scripts/integration_gate.sh`.
5. Push only with user authorization. `.githooks/pre-push` independently checks
   incoming PRs, cleanliness, remote ancestry, development version, and the
   full integration gate.

## Promote to main

1. Confirm the candidate is approved and `develop` CI is green.
2. Create `release/X.Y.Z` from that exact `develop` commit. Never put a stable
   version on `develop`.
3. On the release branch, run
   `bun run scripts/development_version.ts finalize`, then rerun
   `scripts/integration_gate.sh` and commit only the finalization change.
4. Open `release/X.Y.Z` to `main`, wait for required review, and merge using
   repository policy. The resulting `main` push runs CI.
5. Fast-forward local `main`, create an annotated `vX.Y.Z`, prove it is
   reachable from `main`, and prove its manifest version matches the tag.
6. Show the evidence and request approval before pushing the tag. Monitor the
   release workflow and publish only with separate authorization.

If a pushed tag is wrong, stop and request direction. Do not rewrite published
history or delete remote tags automatically.
