# Docker publishing

The [Container workflow](../.github/workflows/container.yml) publishes the same
`linux/amd64` build to GHCR (`ghcr.io/skitsanos/ironflow`) and Docker Hub
(`docker.io/skitsanos/ironflow`).

## Credentials and repository setup

Create the public `skitsanos/ironflow` Docker Hub repository. The workflow
authenticates to GHCR with its built-in `GITHUB_TOKEN` and to Docker Hub as
`skitsanos` with the GitHub repository Actions secret `DOCKERHUB_TOKEN`.
Configure that secret with a Docker Hub access token that has **Read & Write**
repository permission before running the updated workflow. Do not put the token
in workflow files, documentation, or command-line arguments.

Copy [DOCKER_HUB_OVERVIEW.md](DOCKER_HUB_OVERVIEW.md) into the Docker Hub repository
overview when setting it up, and update it when the public container contract
changes. Overview updates are manual; the workflow publishes images only.

## Tags and synchronization

Both registries use these tags:

| Tag | Source |
| --- | --- |
| `latest` | Published builds from `main`. |
| `develop` | Published builds from `develop`. |
| `sha-<full-commit>` | Every published source revision. |
| `X.Y.Z` | The published image for a successful stable `vX.Y.Z` release. |

Build-related pushes to `main` or `develop`, and manual Container workflow runs,
trigger publication. A manual run updates a branch alias only when its selected
ref is that branch. Existing GHCR images are not backfilled automatically.

The existing container storage acceptance step precedes publication. One publish
step sends the same build, SBOM, and provenance to both registries. The dependency
build cache stays on GHCR under `buildcache-amd64` and is not mirrored to Docker Hub.
A successful run records both immutable image references in its **Published
container** summary.

Registry pushes are not atomic across services. If one push fails, rerun the same
workflow revision and confirm both references before treating the aliases as
synchronized. For reproducible deployments, select a successful run and pin its
digest. The OpenShift canary renderer continues to require a GHCR reference;
see [replica deployment](REPLICA_DEPLOYMENT.md).

## Release version tags

After all binary archives and the GitHub Release have been published, the
[Release workflow](../.github/workflows/release.yml) calls
[Container release tags](../.github/workflows/container-release.yml). It promotes
the existing `sha-<release-commit>` image to `X.Y.Z` on both registries without
rebuilding or moving `latest` or `develop`. The complete OCI index, including its
SBOM and provenance, keeps the same digest. The release tag must be a stable
`vX.Y.Z` tag reachable from `main`, and its manifest version must match.

Promotion waits up to five minutes per source reference for Container publication.
Missing images fail the job after that bounded wait; rerun it after Container
succeeds. Source digests must match across both registries. Before changing either
destination, the script rejects an existing version tag with a different digest;
rerunning a matching version is safe. A partial registry failure can still require
a rerun to finish the second destination.

For an existing release, dispatch the promotion workflow with an explicitly
verified digest already present in both registries:

```bash
gh workflow run container-release.yml --ref main \
  -f release_tag=v1.20.0 \
  -f source_digest=sha256:642be0b8776b021e11606454124e5819ad703c05713d6a049f2c085a09dffc77
```

The explicit digest's source revision must be reachable from `main`, have the
same package version, and differ from the release tag only in README,
documentation, or GitHub workflow files. This supports a verified image published
after a documentation-only release follow-up. It never changes the Git release
tag. Leave `source_digest` empty to promote the image for the exact release commit.
