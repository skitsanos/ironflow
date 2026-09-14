# IronFlow product roadmap

This document is the source of truth for product sequencing. The canonical
engineering backlog remains [`issues/README.md`](issues/README.md): every
committed `Now` or `Next` initiative has an active issue record with its exact
acceptance contract. Completed implementation history is retained in
[`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md).

Roadmap horizons describe confidence, not promised dates:

- **Now** — the next bounded goal selected for implementation;
- **Next** — accepted follow-up work whose contract may still be refined; and
- **Later** — useful candidates that are not yet commitments.

## Product posture

IronFlow is a lightweight, self-hosted Rust workflow and ETL engine scripted in
Lua. Its production foundation includes deterministic DAG execution, bounded
resource controls, durable state and events, scheduled and webhook admission,
replica-safe ownership, graceful lifecycle behavior, immutable container
delivery, and live Railway/OpenShift evidence.

That makes IronFlow suitable as an enterprise-deployable engine inside an
operator-controlled trust domain. It does not currently claim to be a hosted
multi-tenant control plane: callers share one API credential, workflows run
with the deployment's process permissions, artifact retention is
operator-owned, and no bundled administrative UI or identity/RBAC layer is
provided.

## Capability maturity

| Capability | Current maturity | Roadmap boundary |
|---|---|---|
| Workflow execution and Lua validation | Production-ready | Continue focused correctness and sandbox regressions |
| JSON, SQLite, PostgreSQL, and Redis state/events | Production-ready within documented backend contracts | Redis Cluster and external event brokers are not current commitments |
| Multi-replica lifecycle | Verified with Docker, Railway, and OpenShift | Deployments must use shared durable state/event stores |
| Artifact handoff | Streamed, integrity-verified local or S3-compatible content-addressed store with offline retained-reference pruning | Deployments must share either a durable mount or one authorized object namespace |
| Operational visibility | Health probes, structured logs, run timing, SSE, and opt-in bounded OpenMetrics | Metrics are process-local and require direct replica scraping; no dashboard or hosted telemetry is bundled |
| Access control | One deployment API key plus network/secret-manager controls | User identity, RBAC, and attributed audit are an explicit deployment boundary |
| Product UI | CLI and REST API only | A read-only visualization slice remains a later candidate |

## Now

_No committed initiative._

## Next

_No committed initiative._

## Later

- **Read-only Web UI** — visualize flow DAGs, current and historical runs, task
  outcomes, and retained events. Promote this to an `IF-NNN` issue only after
  its API/query, authentication, and deployment boundaries are specified.
- **Identity and policy integration** — evaluate scoped credentials, external
  identity proxies, RBAC, and attributed audit only after identifying a
  concrete deployment model that cannot be handled at ingress.
- **Additional distributed backends** — evaluate Redis Cluster key-slot
  compatibility and NATS/Kafka/Redpanda event delivery from demonstrated
  workload requirements rather than adding infrastructure speculatively.

## Explicit non-goals and boundaries

- IronFlow does not provide a hosted SaaS control plane or tenant billing.
- Crash recovery fences abandoned work and records it as stalled; it does not
  transparently replay arbitrary side-effecting workflows.
- Workflows are trusted code with the process permissions granted to the
  deployment. The Lua sandbox and node limits are defense-in-depth controls,
  not hostile multi-tenant process isolation.
- A later item is not implementation-ready until it has an active issue page,
  concrete acceptance criteria, and an identified validation environment.

## Promotion and completion rules

1. Promote at most one tightly bounded initiative into `Now`.
2. Create or refine its canonical `docs/issues/IF-NNN.md` record before code
   changes begin.
3. Keep roadmap language outcome-oriented; implementation details and exact
   regression evidence belong in the issue page.
4. Move completed initiatives out of the active horizons when their issue is
   resolved and preserve their evidence in the issue registry and delivered
   baseline.
5. Reassess `Next` and `Later` after each completed goal rather than treating
   their ordering as a permanent promise.
