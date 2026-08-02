# OpenShift replica canary

This directory contains a namespace-scoped acceptance deployment, not a
production database topology. It creates resources whose names begin with
`ironflow-canary` and leaves credentials outside the manifest.

## Prepare the namespace and secret

Select a disposable project and confirm that the current identity can create
builds, deployments, routes, PVCs, secrets, and disruption budgets:

```bash
oc project <project>
oc auth can-i create builds.build.openshift.io
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

## Deploy and build

Apply the resources, then upload only the files used by the Docker build. This
avoids sending a local `target/`, `.git/`, samples, or unrelated worktree data
to the remote builder. OpenShift's ImageStream trigger rolls out the Deployment
when the build publishes `ironflow-canary:latest`.

```bash
oc apply -f deploy/openshift/canary.yaml

source_dir="$(mktemp -d "${TMPDIR:-/tmp}/ironflow-canary-source.XXXXXX")"
cp Cargo.toml Cargo.lock Dockerfile "$source_dir"/
cp -R src "$source_dir"/

oc start-build ironflow-canary \
  --from-dir="$source_dir" \
  --follow \
  --wait

oc rollout status deployment/ironflow-canary --timeout=10m
```

The first hosted build has a cold Cargo cache and can take more than ten
minutes. A registry-built release image is preferable for repeat deployments;
the binary BuildConfig is a self-contained acceptance fallback.

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
oc delete -f deploy/openshift/canary.yaml --ignore-not-found
oc delete secret ironflow-canary-secrets --ignore-not-found
oc delete builds.build.openshift.io \
  -l buildconfig=ironflow-canary \
  --ignore-not-found
```

Remove the local temporary source directory separately after verifying its
resolved path.
