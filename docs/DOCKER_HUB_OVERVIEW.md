# IronFlow

IronFlow is a Rust workflow engine with Lua workflow definitions, a REST API,
parallel DAG execution, retries, and durable run history.

- [Source code](https://github.com/skitsanos/ironflow)
- [Getting started](https://github.com/skitsanos/ironflow/blob/main/docs/GETTING_STARTED.md)
- [Node documentation](https://github.com/skitsanos/ironflow/tree/main/docs/nodes)
- [Releases](https://github.com/skitsanos/ironflow/releases)

## Images and tags

Docker Hub: `skitsanos/ironflow`

GitHub Container Registry: `ghcr.io/skitsanos/ironflow`

Both registries receive the same build with matching tags:

| Tag | Meaning |
| --- | --- |
| `latest` | Most recently published build from `main`. |
| `develop` | Most recently published development build; may include unreleased changes. |
| `1.20.0` | Published image for the stable 1.20.0 release. |
| `sha-<full-commit>` | A specific published source revision. |

Check the repository's Tags tab for available images. Stable `X.Y.Z` tags are
added automatically after release publication and preserve the promoted digest.
`latest` follows `main`, rather than the GitHub Releases API; check `--version`
when matching an image to a binary release. Use an image digest for reproducible
deployments. The GHCR `buildcache-amd64` tag is build cache, not an application image.

Images include PostgreSQL and Redis storage support and currently target
**linux/amd64**. ARM hosts need x86-64 emulation; Windows hosts need Linux containers.

## Start a local API server

With Docker running, use Bash or Zsh:

```bash
docker run --rm --pull always --name ironflow --platform linux/amd64 \
  -p 127.0.0.1:3000:3000 \
  -e IRONFLOW_ALLOW_UNAUTHENTICATED_API=true \
  -e IRONFLOW_STORE_DIR=/data/state/runs \
  -v ironflow-data:/data \
  skitsanos/ironflow:latest
```

The `ironflow-data` volume preserves run history after the container stops.
In another terminal, check readiness:

```bash
curl --fail http://127.0.0.1:3000/health/ready
```

This localhost example allows unauthenticated access. For hosted deployments,
configure [API authentication](https://github.com/skitsanos/ironflow/blob/main/docs/CLI_REFERENCE.md#api-authentication)
and follow the [deployment guide](https://github.com/skitsanos/ironflow/blob/main/docs/REPLICA_DEPLOYMENT.md).

## License

[MIT](https://github.com/skitsanos/ironflow/blob/main/LICENSE)
