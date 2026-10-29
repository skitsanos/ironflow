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
5. Read active canonical issues and recent review findings before declaring the
   candidate ready. Green CI does not resolve known confidentiality, availability,
   or silent-corruption defects; surface those release risks explicitly.
6. Read [Docker publishing](../../../docs/DOCKER_PUBLISHING.md) and confirm the
   release workflow still calls `container-release.yml` after binary publication.
   Check that the repository Actions secret `DOCKERHUB_TOKEN` is configured;
   inspect secret names only, never credential values. GHCR uses `GITHUB_TOKEN`
   with `packages: write` permission.

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
`.githooks/pre-push` independently checks incoming PRs, worktree cleanliness,
remote ancestry, the development version, and the integration gate receipt for
the pushed tree (run the gate first; it cannot finish inside the push); never
set `IRONFLOW_PREPUSH_GATE=fast` for a release candidate.

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
5. Confirm the CI run for the exact `main` release commit passed. There is no
   Windows cache-primer job; release builds manage their own per-target and
   per-variant dependency caches, and cache hits are not an acceptance gate.
6. Update local `main` with a fast-forward pull. Create an annotated `vX.Y.Z`
   tag only on the verified release commit.
7. Prove the tag is reachable from `main` and its `Cargo.toml` contains the same
   version. Show this evidence before pushing the tag. Authorization must cover
   the tag push and its automatic binary, GHCR, and Docker Hub publication; ask
   only for actions not already authorized in the session.
8. After authorization, push the tag and monitor the complete Release workflow,
   including its container version-tag job. Confirm all four targets produced
   both default and `-full` archives and that the published checksum manifest
   covers all eight archives. Each variant builds its own binary from the tagged
   source. Complete container acceptance below before declaring the release done.

## 6. Publish and verify container version tags

The Release workflow calls
[`container-release.yml`](../../../.github/workflows/container-release.yml)
after the binary archives and GitHub Release are published. Its
[`tag_container_release.ts`](../../../scripts/tag_container_release.ts) script
promotes an existing image without rebuilding it:

- Source: `sha-<full-release-commit>` on GHCR and Docker Hub, published by a
  successful Container workflow for the exact tagged `main` revision.
- Destinations: `ghcr.io/skitsanos/ironflow:X.Y.Z` and
  `docker.io/skitsanos/ironflow:X.Y.Z` for stable Git tag `vX.Y.Z`.
- Both source registries must have the same digest. Promotion preserves the
  complete OCI index, including SBOM and provenance, and leaves branch aliases
  unchanged. Existing version tags with another digest are rejected.

Wait for the Release workflow's container job to succeed. Then inspect the
public registries independently; workflow success alone is not acceptance.
For the approved version, compare these four references:

```bash
VERSION=X.Y.Z
RELEASE_SHA=$(git rev-parse --verify "refs/tags/v${VERSION}^{commit}")
for image in ghcr.io/skitsanos/ironflow docker.io/skitsanos/ironflow; do
  for tag in "$VERSION" "sha-$RELEASE_SHA"; do
    docker buildx imagetools inspect "$image:$tag" --format '{{json .Manifest}}'
  done
done
```

Require all four full root `digest` values to equal the immutable source digest
recorded by the successful Container run. Compare OCI index digests, not only
the runnable platform manifest digest or abbreviated hashes. Check the source
image's `org.opencontainers.image.revision` label against `RELEASE_SHA` and its
source manifest's package version against `X.Y.Z`. The OCI version label may
retain the original alias because promotion does not rewrite image metadata.
`latest` may have advanced since this release; its current digest is not the
source of truth for an older version.

Record the version, Git release SHA, source image revision, Container and Release
run URLs, both version-tag references, and the full matching digest in the release
report or receipt. Distinguish binary checksums, image publication, and deployed
runtime evidence. Missing tags or mismatched digests leave container publication
incomplete even when all binary assets are available.

If only container promotion fails, inspect that job and wait for the source
Container run to succeed. Within the authorized publication scope, rerun the
failed container job or dispatch `container-release.yml` on `main` with
`release_tag=vX.Y.Z` and no `source_digest`. Do not rerun successful binary jobs
just to repair image tagging. A conflicting existing version digest requires
stopping and reporting the conflict rather than overwriting or deleting the tag.

For an explicitly authorized backfill, use the documented `source_digest`
override with an independently verified digest already present on both
registries. The script requires matching package versions, `main` ancestry, and
source differences limited to README, docs, or GitHub workflow files. Compare
both resulting version tags to that approved digest and record the actual image
source revision; do not require it to equal the historical Git release SHA in
this documented exception. Never select a moving `latest` implicitly, rebuild
the image to obtain a new digest, or move the Git release tag for a backfill.

If an unpushed local release step is wrong, use a new corrective edit or ask
before any destructive recovery. Once a tag is pushed, stop and request
direction rather than deleting or rewriting published history.
