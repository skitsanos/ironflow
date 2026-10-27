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

One metadata step generates the same tags for both registries:

| Tag | Source |
| --- | --- |
| `latest` | Published builds from `main`. |
| `develop` | Published builds from `develop`. |
| `sha-<full-commit>` | Every published source revision. |

Build-related pushes to `main` or `develop`, and manual Container workflow runs,
trigger publication. A manual run updates a branch alias only when its selected
ref is that branch. GitHub release tags do not trigger this workflow or create
version-number image tags. Existing GHCR images are not backfilled automatically.

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
