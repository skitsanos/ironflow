# OpenShift replica canary

This directory contains a namespace-scoped acceptance deployment, not a
production database topology. It creates resources whose names begin with
`ironflow-canary`, leaves credentials outside the manifest, and requires an
immutable GHCR image reference. Never apply `canary.yaml` directly: its image
marker is intentionally not pullable.

## Prepare the namespace and secret

Select a disposable project and confirm that the current identity can create
deployments, routes, PVCs, secrets, and disruption budgets:

```bash
oc project <project>
oc auth can-i create deployments.apps
oc auth can-i create routes.route.openshift.io
oc auth can-i create persistentvolumeclaims
```

Generate the temporary values without printing them, then stream a JSON Secret
directly to the API. Do not run this shell with command tracing enabled.

```bash
export IF_CANARY_DB_PASSWORD="$(openssl rand -hex 24)"
export IF_CANARY_API_KEY="$(openssl rand -hex 32)"

bun -e '
const password = Bun.env.IF_CANARY_DB_PASSWORD;
const apiKey = Bun.env.IF_CANARY_API_KEY;
if (!password || !apiKey) throw new Error("missing generated canary secret");
process.stdout.write(JSON.stringify({
  apiVersion: "v1",
  kind: "Secret",
  metadata: {
    name: "ironflow-canary-secrets",
    labels: { "app.kubernetes.io/part-of": "ironflow-openshift-canary" },
  },
  type: "Opaque",
  stringData: {
    "database-password": password,
    "database-url": `postgres://ironflow:${password}@ironflow-canary-postgres:5432/ironflow`,
    "api-key": apiKey,
  },
}));
' | oc apply -f -

unset IF_CANARY_DB_PASSWORD IF_CANARY_API_KEY
```

## Select and deploy an immutable image

The Container workflow publishes `linux/amd64` images to GHCR for pushes to
`develop` and `main`. Every build receives an immutable
`sha-<40-character-commit>` tag and an OCI digest; no mutable branch or latest
tag is published. Resolve the digest without pulling the image, then render the
canary template. The renderer rejects tags, other repositories, malformed
digests, and missing or duplicate template markers.

```bash
commit_sha="$(git rev-parse HEAD)"
image_tag="ghcr.io/skitsanos/ironflow:sha-${commit_sha}"
image_digest="$(docker buildx imagetools inspect \
  "$image_tag" --format '{{.Manifest.Digest}}')"
image_ref="ghcr.io/skitsanos/ironflow@${image_digest}"

bun run deploy/openshift/render.ts "$image_ref" \
  | oc apply -f -

oc rollout status deployment/ironflow-canary --timeout=5m
```

Confirm that both the declared image and the runtime image ID contain the same
digest. The first command verifies intent; the second verifies what the runtime
actually started.

```bash
oc get deployment ironflow-canary \
  -o jsonpath='{.spec.template.spec.containers[?(@.name=="ironflow")].image}{"\n"}'
oc get pods \
  -l app.kubernetes.io/name=ironflow,app.kubernetes.io/instance=canary \
  -o jsonpath='{range .items[*]}{.metadata.name}{" "}{.status.containerStatuses[?(@.name=="ironflow")].imageID}{"\n"}{end}'
```

## Inspect the admitted workload

Do not infer runtime confinement solely from the requested YAML. Inspect each
admitted pod and verify its SCC, UID, security context, capability mask, and
mounts:

```bash
oc get pods \
  -l app.kubernetes.io/name=ironflow,app.kubernetes.io/instance=canary \
  -o wide

oc get pod <pod> \
  -o jsonpath='{.metadata.annotations.openshift\.io/scc}{"\n"}'
oc exec <pod> -- id
oc exec <pod> -- grep '^CapEff:' /proc/1/status
oc exec <pod> -- awk '$2 == "/" { print $4 }' /proc/mounts

route_host="$(oc get route ironflow-canary -o jsonpath='{.spec.host}')"
curl --fail "https://${route_host}/health/live"
curl --fail "https://${route_host}/health/ready"
```

Use `examples/22-replica-deployment/hold.lua` for lifecycle tests. Submit it
directly to one pod with a bounded client timeout and an `Idempotency-Key`, then
read the deterministic run from the other pod. A normal pod deletion should
produce `cancelled`; an intentionally forced deletion should leave the run
`running` until its lease expires, then a peer should mark it `stalled` within
the documented 90–120 second window. Confirm the Route and peer remain ready
throughout. Forced deletion is disruptive and belongs only in a disposable
canary namespace.

## Cleanup

Delete only the named canary resources; do not delete the project or unrelated
workloads:

```bash
oc delete \
  deployment/ironflow-canary \
  deployment/ironflow-canary-postgres \
  service/ironflow-canary \
  service/ironflow-canary-postgres \
  route.route.openshift.io/ironflow-canary \
  poddisruptionbudget.policy/ironflow-canary \
  persistentvolumeclaim/ironflow-canary-postgres \
  secret/ironflow-canary-secrets \
  --ignore-not-found
```
