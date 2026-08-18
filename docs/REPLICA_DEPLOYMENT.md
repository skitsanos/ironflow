# Replica deployment and failure contract

IronFlow replicas are active-active API and scheduler processes sharing durable
state. Replica mode is deliberately explicit:

```yaml
replica_mode: true
store_backend: postgres
event_store: postgres
```

Supply `IRONFLOW_STORE_URL` and `IRONFLOW_EVENT_STORE_URL` through the platform
secret manager; YAML values are literal and do not expand environment syntax.

`IRONFLOW_REPLICA_MODE=true` is the environment equivalent. Startup fails when
replica mode selects JSON, SQLite, or in-memory events. PostgreSQL and Redis are
the supported shared backends. Do not mount a JSON/SQLite directory into
multiple containers and treat it as a distributed database.

Artifact-producing deployments must also choose one cross-replica contract.
Either mount the same durable `IRONFLOW_ARTIFACT_DIR` on every replica, or set
`IRONFLOW_ARTIFACT_BACKEND=s3` with one shared bucket/prefix and workload
credentials. S3 mode keeps only private bounded staging/cache on each replica;
the content-addressed remote object is authoritative, so another replica can
restore and verify it without sharing a filesystem. Keep the local cache on a
writable volume or `/tmp` in restricted/read-only-root containers.

## Failure semantics

An executing process owns its run through a fenced durable lease. A live owner
renews every 30 seconds; the lease expires after 90 seconds. If the process is
killed, a peer keeps serving and eventually marks the abandoned run `Stalled`.
The worst ordinary detection window is about 120 seconds: one lease lifetime
plus one reaper interval, excluding a storage outage.

IronFlow does not replay a `Stalled` workflow automatically. Nodes may have
already sent mail, charged an API, or changed another database. Fencing prevents
late owner writes to IronFlow state; it cannot undo or deduplicate side effects
in external systems.

`POST /flows/run` accepts an optional `Idempotency-Key` header containing 1–128
ASCII letters, digits, `-`, `_`, `.`, or `:`. The raw key is never stored. Its
digest identifies the durable run, and a request fingerprint is committed with
that run. Concurrent or later same-key/same-request submissions return the same
run. Reusing the key for another source, file name, or context returns HTTP 409.
The identity is retained for the lifetime of the run record; explicitly deleting
the run also releases that identity.

Scheduled occurrences derive their durable run ID from the schedule name and
resolved instant. A replica that observes an existing claim still converges on
that run ID. This closes the claim-to-start crash gap while retaining exactly
one run record. It does not replay an occurrence whose existing run later
becomes `Stalled`.

## Health and shutdown

| Endpoint | Meaning |
| --- | --- |
| `GET /health/live` | Process is alive; does not probe storage |
| `GET /health/ready` | Process accepts execution and both configured stores answer a bounded probe |
| `GET /health` | Backwards-compatible liveness alias |
| `GET /metrics` | Optional process-local OpenMetrics; registered only when enabled and protected like the API |

Enable metrics on every replica with `IRONFLOW_METRICS_ENABLED=true` and scrape
each pod directly. Counters and histograms reset when that process restarts, so
a load-balanced scrape does not represent the deployment. The Prometheus
Operator templates and alert examples are documented in
[Operator metrics](METRICS.md).

SIGTERM and SIGINT close readiness and scheduler/API execution admission first.
HTTP listener shutdown then begins while already accepted runs retain their
admission permits. IronFlow waits `IRONFLOW_SHUTDOWN_GRACE_SECONDS` (default 30,
maximum 3600), cooperatively cancels remaining runs, allows up to 20 seconds for
their durable finalization, and then bounds connection shutdown by another five
seconds. Configure the platform termination window above that total.

## Local Docker acceptance

The opt-in gate builds the production image, starts PostgreSQL, two restricted
IronFlow containers, and an nginx round-robin proxy. It verifies simultaneous
cold start, cross-replica state, idempotent retry, a producer-on-A and
consumer-on-B streamed artifact handoff through disposable MinIO, schedule
uniqueness, SIGTERM draining, real SIGKILL lease reconciliation, arbitrary UID
execution, and a read-only root filesystem.

```bash
IRONFLOW_RUN_REPLICA_ACCEPTANCE=1 bun run scripts/replica_acceptance.ts
```

The project is explicitly named `ironflow-replica-acceptance`; cleanup removes
only its containers, network, and disposable PostgreSQL volume. The locally
cached images remain available. The gate takes more than two minutes because it observes the production
90-second lease rather than substituting a test-only timeout.

Docker evidence does not prove Railway edge routing, OpenShift Route behavior,
Security Context Constraint selection, storage service availability, or
platform-specific rollout timing. Validate those with a canary deployment.

## Railway

- The repository `railway.json` selects the Dockerfile, checks
  `/health/ready`, overlaps deployments for five seconds, and gives SIGTERM 60
  seconds before Railway sends SIGKILL.
- Run at least two replicas with PostgreSQL or Redis state and durable events.
- Point both `IRONFLOW_STORE_URL` and `IRONFLOW_EVENT_STORE_URL` at a private
  reference variable such as `${{Postgres.DATABASE_URL}}`; do not copy the
  rendered credential into source or a local deployment file.
- Set `IRONFLOW_REPLICA_MODE=true` and do not attach a Railway volume to the
  IronFlow service.
- Use `/health/ready` as the deployment healthcheck.
- Set the deployment drain window to at least
  `IRONFLOW_SHUTDOWN_GRACE_SECONDS + 25` seconds; 60 seconds fits the default.
- Use continuous external monitoring in addition to Railway's deployment-time
  healthcheck.
- When deploying the prebuilt public GHCR image instead of a repository build,
  configure the service source with the exact `ghcr.io/skitsanos/ironflow@sha256:...`
  reference produced by the Container workflow. Do not substitute `develop`,
  `main`, or `latest`, and do not enable image auto-updates for a digest-pinned
  canary.

For a canary, first deploy the same revision as two independently addressable
services sharing the database. That makes cross-process create/read and
idempotency assertions deterministic. Then temporarily scale one service to two
Railway replicas and use Railway HTTP telemetry's deployment-instance IDs to
confirm that requests reached both instances. Restore the intended scale after
the test. Never print `railway variable list --json` or `--kv` output in logs;
both forms contain rendered secret values.

## OpenShift

The image's writable paths are root-group writable and it can run with the
arbitrary UID assigned by OpenShift's restricted SCC. Do not set a fixed
`runAsUser` solely for IronFlow and do not grant `anyuid`.

[`deploy/openshift/canary.yaml`](../deploy/openshift/canary.yaml) is a
namespace-scoped acceptance template: a disposable PostgreSQL PVC, two
IronFlow replicas, health probes, edge-TLS Route, and a PodDisruptionBudget. It
intentionally omits credentials and contains a non-pullable image marker. The
repository Container workflow publishes `linux/amd64` images to GHCR with an
immutable tag for every commit on `develop` and `main`; deployment renders the
template with the runnable `linux/amd64` manifest digest selected from the
attested OCI index, never the index itself or a branch tag. That makes the
declared Deployment digest identical to each container runtime `imageID` while
the index retains the SBOM and provenance attestations. The workflow's mutable
`buildcache-amd64` GHCR tag stores zstd-compressed BuildKit state only and is
never a runnable deployment reference. Create the referenced
`ironflow-canary-secrets` Secret through the platform secret boundary before
applying the rendered manifest. See
[`deploy/openshift/README.md`](../deploy/openshift/README.md) for digest
resolution, rendering, inspection, failure, and exact cleanup procedures.

The production pod policy is equivalent to:

```yaml
spec:
  terminationGracePeriodSeconds: 40
  securityContext:
    seccompProfile: { type: RuntimeDefault }
  containers:
    - name: ironflow
      securityContext:
        runAsNonRoot: true
        allowPrivilegeEscalation: false
        readOnlyRootFilesystem: true
        capabilities:
          drop: ["ALL"]
      readinessProbe:
        httpGet: { path: /health/ready, port: 3000 }
      livenessProbe:
        httpGet: { path: /health/live, port: 3000 }
      startupProbe:
        httpGet: { path: /health/ready, port: 3000 }
        failureThreshold: 60
        periodSeconds: 2
```

The canary uses a 5-second application shutdown grace, so the 40-second pod
window covers cancellation finalization and bounded connection shutdown. Keep
the platform window at least `IRONFLOW_SHUTDOWN_GRACE_SECONDS + 25` seconds when
using another value. `/tmp` is a size-limited memory-backed volume; the root
filesystem remains read-only. Mount other writable data only when a node
explicitly needs it. Shared run/event state still belongs in PostgreSQL or
Redis.

On 2026-08-02, the manifest was exercised in a shared Red Hat Developer Sandbox
running OpenShift 4.21. The admitted pods used `restricted-v2`, namespace UID
`1004800000`, `RuntimeDefault` seccomp, an effective capability mask of zero,
and a read-only root mount. The edge Route stayed ready across graceful and
forced pod replacement. PostgreSQL-backed create/read and idempotent retry
worked across two pods; graceful deletion cancelled the owned run, while a
forced deletion left it for peer reconciliation and it became `stalled` after
about 110 seconds.

On 2026-08-03, the same sandbox pulled the public GHCR artifact published for
commit `6694f091cb6fdd30690e5c22ada52b01b0ec756f`. The Deployment and both
runtime `imageID` values matched the selected `linux/amd64` manifest digest
`sha256:435a3c3ab154b9ca0454d2eac91275e8708204c18cded7e18a68cf74658f261f`
exactly. The restricted-SCC, Route, shared-state, and idempotency checks still
passed, and forced owner death reached `stalled` after 94 seconds. The named
canary resources were removed afterward; the project and unrelated workloads
were retained.

That canary proves the tested namespace, SCC, Route, and process lifecycle. It
does not prove another cluster's operators, quotas, network policies, storage
class, ingress configuration, or long-duration availability. A PDB protects
against supported voluntary disruptions; it does not prevent direct or forced
pod deletion, node loss, or simultaneous infrastructure failure.
