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
replica mode selects JSON, SQLite, or in-memory events. Selecting `postgres`
also requires the corresponding state/event URL to start with `postgres://`
or `postgresql://`; a SQLite URL under a PostgreSQL label is rejected. Startup
preflights both selected SQL URLs before either store connects or creates local
database files. Environment values override YAML, including an invalid or empty
value, which fails instead of falling back. Errors identify the setting without
echoing its URL. PostgreSQL and Redis are the supported shared backends.

For builds containing IF-137, PostgreSQL features include Rustls TLS and Redis
features include verified `rediss://` connections. Prefer PostgreSQL
`sslmode=verify-full` with `sslrootcert`, and configure Redis native trust roots
or `SSL_CERT_FILE` for a private CA. Published v1.18.1 and earlier binaries lack
these store TLS features. Do not work around that limitation by disabling
required encryption or certificate verification. See
[encrypted shared stores](CLI_REFERENCE.md#encrypted-shared-stores).
Do not mount a JSON/SQLite directory into
multiple containers and treat it as a distributed database.

The URL check verifies the selected dialect, not deployment topology. Operators
must still point every replica at the intended shared services and table/key
namespaces; two unrelated PostgreSQL databases cannot be detected from their
URL schemes. Ordinary standalone SQLite remains supported with `sqlite:` URLs
or its default local paths.

### PostgreSQL startup and schema locks

A replica that starts against an up-to-date PostgreSQL schema requests no
table lock. It reads the schema from the catalog and repairs only event
counters that are missing or behind, so it neither waits for a dump, a long
query or an idle transaction, nor delays the replicas that are serving. Scale
out, restart and roll replicas without a maintenance window.

A start that has to change the schema is different: the first start after an
upgrade that adds an index or a column, or the one-time event identity
migration from a legacy `(id)` key. Such a change needs a table lock, and
PostgreSQL queues every later statement on that table behind a lock request
that is waiting. IronFlow therefore waits at most 5 seconds
(`lock_timeout`; the bound is fixed on purpose and not configurable, because
it is also the longest one start attempt can stall the replicas that are
serving) and then stops with an error of this form:

```text
Timed out after 5 s waiting for a lock on PostgreSQL table 'ironflow_events'
for the one-time event identity migration. Another session holds a
conflicting lock (a dump, a long query or an idle transaction). Nothing was
changed; end that session and start again
```

Nothing was changed and the next start retries, so an orchestrator restart
loop converges once the blocking session ends. To find that session, look for
old transactions in `pg_stat_activity` (`xact_start`, `state = 'idle in
transaction'`) or for granted locks on the table in `pg_locks`. During each
attempt, statements of running replicas on that table can wait up to the same
5 seconds; event publication gives up after 500 ms, so a run that publishes
during an attempt can lose live events while its state stays correct. Apply
schema-changing upgrades when no long transaction is open. While one replica
builds a missing index on a large table, another starting replica hits the
same timeout, with an error that names the index, and succeeds on a later
start. The identity migration still
requires the coordinated writer stop described in the
[architecture](ARCHITECTURE.md).

Artifact-producing deployments must also choose one cross-replica contract.
Either mount the same durable `IRONFLOW_ARTIFACT_DIR` on every replica, or set
`IRONFLOW_ARTIFACT_BACKEND=s3` with one shared bucket/prefix and workload
credentials. S3 mode keeps only private bounded staging/cache on each replica;
the content-addressed remote object is authoritative, so another replica can
restore and verify it without sharing a filesystem. Keep the local cache on a
writable volume or `/tmp` in restricted/read-only-root containers.

The S3 backend can use an MRAP ARN as `IRONFLOW_ARTIFACT_S3_BUCKET` with SigV4a.
Keep path-style addressing disabled and leave custom endpoints unset; see
[S3 signing](SECURITY.md#s3-signing) for AWS restrictions and validation limits.
This is not a drop-in consistency guarantee: MRAPs do not replicate objects
automatically, and a routed read can return 404 before the destination bucket
has the object. For immediate cross-replica visibility, use one regional bucket.
MRAP deployments need separately qualified replication/routing and offline
pruning coordination.

### Scheduling artifact pruning

Nothing evicts artifacts automatically, so schedule
[`ironflow artifacts prune`](CLI_REFERENCE.md#ironflow-artifacts-prune) in a
maintenance window. It is offline-only: stop every replica and every other
writer of the shared state and artifact stores first, run it from one host
with the same store and artifact configuration as the replicas, and restart
afterwards. A referenced artifact is never deleted, but a reference written
while the command runs is not seen.

One invocation is bounded (at most `--scan` stored digests examined, default
10,000, and at most `--limit` deletions, default 100) and prints either
`Artifact prune complete` or `Artifact prune incomplete: … continue with
--after <sha256>`. A single invocation per window is not enough once the store
outgrows `--scan`: loop, passing each printed digest back, until the command
prints `complete`, as in the script in the CLI reference. Each step resolves
everything up to the digest it prints, so the loop finishes in at most
⌈digests / scan⌉ + ⌈deletable / limit⌉ invocations and can be cut short and
resumed in the next window with the last printed digest. Size the window from
those two numbers and from the number of retained runs, which every invocation
walks once. Alert on a non-zero exit status and on the standard-error line
about non-canonical `artifact://` strings; `deleted 0` on its own is normal
for a window in which every old artifact is still referenced.

## Empty variables

Reference variables and secret references can resolve to an empty string: a
Railway reference to a variable that does not exist, a compose
`${NAME}` placeholder with `NAME` undefined, or a Kubernetes or OpenShift
`secretKeyRef` whose key holds no data. IronFlow does not treat such a
variable as unset. A present but empty `IRONFLOW_API_KEY`,
`IRONFLOW_STORE_URL`, `IRONFLOW_EVENT_STORE_URL`, `IRONFLOW_STORE`,
`IRONFLOW_EVENT_STORE`, `IRONFLOW_SQL_TABLE_PREFIX`, `IRONFLOW_CORS_ORIGINS`,
`REDIS_URL` or `REDIS_PREFIX` stops the replica at startup with a message that
names the variable (IF-198), so the rollout fails its readiness check instead
of serving with a dropped key or a different table prefix. Fix the reference,
or remove the variable from the service to use the `ironflow.yaml` value or
the default. In compose, prefer `${NAME:?NAME is required}` for secrets so
the failure appears before the container starts.

## Outbound destination policy

`IRONFLOW_BLOCK_PRIVATE_NETWORK` and `IRONFLOW_PRIVATE_NETWORK_ALLOWLIST` are
process settings: a run is refused or allowed a destination by the replica
that executes the step. Give every replica the same values, or a retried
step can behave differently on another replica. The policy applies to
destinations a flow chooses (HTTP, provider, MCP, S3, SMTP and database
nodes); the shared state store, event store and artifact store of the
deployment are operator configuration and are never checked, so they need
no allowlist entry. A present but empty value of either variable stops the
replica at startup, like the variables above. See
[Outbound destinations](SECURITY.md#outbound-destinations).

## Stored timestamps across versions

Replicas on different IronFlow versions may share a store during a rolling
upgrade. The move of IronFlow's own time handling from `chrono` to `jiff`
(IF-231) changed no stored or published timestamp: text columns, JSON
records, Redis values, run-list cursors and microsecond sort keys keep their
exact bytes, and each implementation reads what the other writes and lists
runs in the same order. No migration or ordering of the upgrade is needed.
[Stored timestamp forms](ARCHITECTURE.md#stored-timestamp-forms) lists the
forms.

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
The fingerprint is set at admission; task output and its size limit cannot
replace it. If an older version already truncated a run's fingerprint, the
original request cannot be verified, and retries still return HTTP 409.
The identity is retained for the lifetime of the run record; explicitly deleting
the run also releases that identity. A later submission with the same key
starts a new run under the same run ID, on any replica, and its events and
SSE stream are complete: the event store's deletion fence rejects only late
events from the deleted run. The key can be reused without waiting for the
`DELETE` response: a deletion that overlaps the new submission, on this or
another replica, removes only the events of the run whose state it deleted.
Every event backend records the run that owns a stream when the run starts,
and an event delete that finds a newer owner changes nothing. During a
rolling upgrade a replica that predates this release still deletes and
fences in the old way, so the guarantee holds once every replica is
upgraded.

Scheduled occurrences derive their durable run ID from the schedule name and
resolved instant. A replica that observes an existing claim still converges on
that run ID. This closes the claim-to-start crash gap while retaining exactly
one run record. It does not replay an occurrence whose existing run later
becomes `Stalled`.

The claim also records that its occurrence produced a run. A replica that
starts or restarts inside the grace window therefore does not start the
instant again after its run record is gone, whether the record expired through
`REDIS_TTL` or was removed with `DELETE /runs/{id}`. Deleting a scheduled run
removes the record on every backend; it does not release the instant. Start
the flow through the API or CLI to run it again. A claim whose replica died
before creating the run carries no such record and is still recovered within
grace.

An occurrence that a replica skipped is settled in the same way. A skip for
lateness, overlap with the schedule's previous run,
`IRONFLOW_MAX_CONCURRENT_RUNS` or a flow that fails to load is recorded in
the claim, so a replica that starts or restarts inside the grace window
reports the instant as `not_claimed` and does not start it later, for example
once the overlapping run has finished. A replica that is draining does not
skip: it defers, records nothing, and the instant is started by a peer or by
the replica after its restart while it is inside grace.

A replica that starts with several missed instants of a schedule inside the
grace window decides only the most recent one and records the earlier ones as
superseded (scheduler outcome `superseded`). Another replica that starts
later inside the same window finds all of them settled and starts none. See
[Catch-up](CLI_REFERENCE.md#catch-up).

Both Redis stores wait `IRONFLOW_REDIS_RESPONSE_TIMEOUT_MS` (default 500) for
each response. If store operations fail with `timed out` under load, raise it
on every replica; a response that arrives late is otherwise reported as a
failed operation although Redis may have applied it.

With the Redis state store, keep `REDIS_TTL` greater than the largest
`grace_seconds` plus 60 seconds. `ironflow serve` logs a warning for each
schedule that is not covered. The occurrence is still not run twice, but its
run record and history expire while the instant is reachable, and a replica
that dies between creating a run and recording it in the claim is covered
only while that run record exists.

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
Already accepted runs retain their admission permits, and the listener stays
open while they drain: a connection opened after the signal is answered at
once, `/health/ready` with `503` and `"status": "draining"`, and
`POST /flows/run`, `POST /flows/validate` and webhooks with `503`, so a probe or a load balancer sees
the replica leave rotation instead of timing out. Run status and event reads
keep working for clients that follow an accepted run. IronFlow waits
`IRONFLOW_SHUTDOWN_GRACE_SECONDS` (default 30, maximum 3600), cooperatively
cancels remaining runs and allows up to 20 seconds for their durable
finalization. Only then does the listener close, after which new connections
are refused; open connections get up to five seconds to finish, and blocking
workers that cannot be interrupted get up to
`IRONFLOW_RUNTIME_SHUTDOWN_SECONDS` more (default 5) before the process
exits without them. With that default the process exits at most
`IRONFLOW_SHUTDOWN_GRACE_SECONDS + 30` seconds after the signal; add the
difference when you raise the runtime bound. Configure the
platform termination window at or above that total.

## Local Docker acceptance

The opt-in gate builds the production image, starts PostgreSQL, two restricted
IronFlow containers, and an nginx round-robin proxy. It verifies simultaneous
cold start, cross-replica state, idempotent retry, a producer-on-A and
consumer-on-B streamed artifact handoff through a disposable S3 gateway
(`versity/versitygw` over a local directory; the MinIO images used before are
no longer published, IF-220), schedule
uniqueness, SIGTERM draining with a `503` readiness answer on new connections,
real SIGKILL lease reconciliation, arbitrary UID
execution, and a read-only root filesystem.

The stack's PostgreSQL image (`deploy/replica/docker-compose.yml`) is the one
major-version tag that the integration gate, the CI storage job and the live
TLS acceptance also run. `scripts/tests/postgres_image.test.ts` fails when the
four disagree, and Renovate proposes the next major for all of them in one
update (IF-263).

```bash
IRONFLOW_RUN_REPLICA_ACCEPTANCE=1 bun run scripts/replica_acceptance.ts
```

The image build links the release binary with fat LTO in one codegen unit.
A Docker VM with 8 GiB of memory was not enough for that link (the compiler
is killed and the build reports `cannot allocate memory`); 12 GiB was
enough. Give the VM that much before running the gate.

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
  `IRONFLOW_SHUTDOWN_GRACE_SECONDS + 30` seconds; 60 seconds fits the default.
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

The image leaves the default store directory absent so a non-root runtime
UID can create and own it with private permissions. Fresh named volumes work
with the default UID or an arbitrary UID in group 0. This does not make a local
store volume portable between UIDs or suitable for replica sharing: use
PostgreSQL or Redis for the replica configuration below. Existing root-owned
store directories require an offline, backed-up ownership migration or a new store
path; an image update alone does not repair their ownership.

The image's writable paths are root-group writable and it can run with the
arbitrary UID assigned by OpenShift's restricted SCC. Do not set a fixed
`runAsUser` solely for IronFlow and do not grant `anyuid`.

[`deploy/openshift/canary.yaml`](../deploy/openshift/canary.yaml) is a
namespace-scoped acceptance template: a disposable PostgreSQL PVC, two
IronFlow replicas, health probes, edge-TLS Route, and a PodDisruptionBudget. It
intentionally omits credentials and contains a non-pullable image marker. The
repository Container workflow publishes `linux/amd64` images to GHCR with an
immutable tag for each published commit on `develop` and `main`, and publishes
the same build and tags to Docker Hub as `skitsanos/ironflow`. The OpenShift
renderer continues to require the GHCR reference; deployment renders the
template with the runnable `linux/amd64` manifest digest selected from the
attested OCI index, never the index itself or a branch tag. That makes the
declared Deployment digest identical to each container runtime `imageID` while
the index retains the SBOM and provenance attestations. The workflow's mutable
`buildcache-amd64` GHCR tag stores BuildKit cache state with `mode=max` and is
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
window covers cancellation finalization, bounded connection shutdown and the
bounded wait for blocking workers. Keep
the platform window at least `IRONFLOW_SHUTDOWN_GRACE_SECONDS + 30` seconds when
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
