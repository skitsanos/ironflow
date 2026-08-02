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

SIGTERM and SIGINT close readiness and scheduler/API execution admission first.
HTTP listener shutdown then begins while already accepted runs retain their
admission permits. IronFlow waits `IRONFLOW_SHUTDOWN_GRACE_SECONDS` (default 30,
maximum 3600), cooperatively cancels remaining runs, allows up to 20 seconds for
their durable finalization, and then bounds connection shutdown by another five
seconds. Configure the platform termination window above that total.

## Local Docker acceptance

The opt-in gate builds the production image, starts PostgreSQL, two restricted
IronFlow containers, and an nginx round-robin proxy. It verifies simultaneous
cold start, cross-replica state, idempotent retry, schedule uniqueness, SIGTERM
draining, real SIGKILL lease reconciliation, arbitrary UID execution, and a
read-only root filesystem.

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

- Run at least two replicas with PostgreSQL or Redis state and durable events.
- Set `IRONFLOW_REPLICA_MODE=true` and do not attach a Railway volume to the
  IronFlow service.
- Use `/health/ready` as the deployment healthcheck.
- Set the deployment drain window to at least
  `IRONFLOW_SHUTDOWN_GRACE_SECONDS + 25` seconds; 60 seconds fits the default.
- Use continuous external monitoring in addition to Railway's deployment-time
  healthcheck.

## OpenShift

The image's writable paths are root-group writable and it can run with an
arbitrary namespace UID. A representative pod policy is:

```yaml
spec:
  terminationGracePeriodSeconds: 60
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
        failureThreshold: 30
        periodSeconds: 2
```

Do not set a fixed `runAsUser` solely for IronFlow and do not grant `anyuid`.
Mount writable data only when a node explicitly needs it; shared run/event
state still belongs in PostgreSQL or Redis.
