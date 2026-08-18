# Operator metrics

IronFlow exposes an opt-in, process-local OpenMetrics endpoint for Prometheus
and compatible collectors. Enable it with either configuration source:

```yaml
metrics_enabled: true
```

```bash
IRONFLOW_METRICS_ENABLED=true ironflow serve
```

`GET /metrics` is not registered when metrics are disabled and returns `404`.
When enabled, it uses the same listener and authentication middleware as the
other protected API routes. A non-loopback server therefore requires
`IRONFLOW_API_KEY`, and a scraper can use either form:

```bash
curl --fail \
  -H "Authorization: Bearer $IRONFLOW_API_KEY" \
  http://127.0.0.1:3000/metrics
```

Loopback keeps the existing local-development exception. Setting
`IRONFLOW_ALLOW_UNAUTHENTICATED_API=true` also makes `/metrics`
unauthenticated; use that only behind an explicit network boundary. There is
no metrics-only credential in this release, so a scraper holding the API key
can authenticate to the rest of the IronFlow API. Keep the key in a secret
manager and restrict scraper-to-pod traffic with platform network policy.

Responses use
`application/openmetrics-text; version=1.0.0; charset=utf-8` and
`Cache-Control: no-store`.

## Metric contract

All labels come from the fixed vocabularies below. IronFlow never uses run IDs,
flow or schedule names, task or node names, URLs, error text, context values,
credentials, or other caller-controlled data as labels.

| Metric | Type | Labels | Meaning |
| --- | --- | --- | --- |
| `ironflow_runs_total` | counter | `outcome` | Terminal runs initialized and executed by this process |
| `ironflow_run_duration_seconds` | histogram | `outcome` | Time from durable run initialization through terminalization |
| `ironflow_task_attempts_total` | counter | `outcome` | Individual task attempts, including retries |
| `ironflow_task_attempt_duration_seconds` | histogram | `outcome` | Individual task-attempt duration |
| `ironflow_active_work` | gauge | `kind` | Work currently owned by this process |
| `ironflow_admission_decisions_total` | counter | `resource`, `decision` | Run and flow-loader admission results |
| `ironflow_scheduler_decisions_total` | counter | `outcome` | Final decision for each due schedule occurrence evaluated by this process |
| `ironflow_lease_events_total` | counter | `outcome` | Run-owner lease renewal and loss events |
| `ironflow_storage_failures_total` | counter | `store`, `operation`, `error_kind` | Errors returned by state and event-store operations |

Label values are stable and bounded:

| Label | Values |
| --- | --- |
| Run `outcome` | `success`, `failed`, `cancelled`, `stalled` |
| Task-attempt `outcome` | `success`, `failed`, `timed_out`, `aborted` |
| Active-work `kind` | `run`, `task`, `flow_load` |
| Admission `resource` | `run`, `flow_load` |
| Admission `decision` | `accepted`, `at_capacity`, `draining` |
| Scheduler `outcome` | `fired`, `not_claimed`, `late`, `overlapped`, `at_capacity`, `failed`, `claim_failed`, `timed_out` |
| Lease `outcome` | `renewed`, `lost`, `timed_out`, `error`, `reconciliation_failed` |
| Storage `store` | `state`, `event` |
| Storage `error_kind` | `invalid_input`, `not_found`, `backend`, `corruption`, `conflict` |

State-store operations are `healthcheck`, `init_run`, `init_run_owned`,
`set_run_status`, `set_run_status_owned`, `renew_run_lease`,
`reconcile_expired_run_leases`, `upsert_task`, `upsert_task_owned`,
`get_context`, `update_context`, `update_context_owned`, `get_run_info`,
`list_runs`, `list_run_summaries`, `list_run_summaries_page`, `delete_run`,
`prune_before`, and `claim_schedule`. Event-store operations are `healthcheck`,
`publish_event`, `delete_run`, and `list_events`.

The histograms use fixed second buckets from 5 ms through 5 minutes. Every
valid series is initialized at startup, so absent activity is represented by
zero rather than a missing label combination.

## Stability and reset semantics

Metric names, types, and existing label meanings are a compatibility contract
within a major IronFlow release. New metrics or new values in an existing
bounded vocabulary may be added in a minor release. Renaming a metric, changing
its type, or changing the meaning of an existing label requires a major
release.

The registry lives in memory and resets on every process restart. Counters and
histograms are not reconstructed from durable run history. Gauges and counters
describe one process, so scrape every replica directly and aggregate in
PromQL. Do not send a scrape through a load-balanced service and treat the
result as cluster-wide state.

Metrics recording performs no disk or network I/O and is not awaited by state
transitions. A metrics failure cannot reject, retry, or change a workflow,
scheduler, lease, or storage decision. Encoding a scrape operates over the
fixed in-memory series set.

Useful replica-aware queries include:

```promql
sum(rate(ironflow_runs_total[5m])) by (outcome)

histogram_quantile(
  0.95,
  sum(rate(ironflow_run_duration_seconds_bucket[10m])) by (le, outcome)
)

sum(rate(ironflow_admission_decisions_total{decision="at_capacity"}[5m]))
```

## Kubernetes and alerts

[`deploy/metrics/servicemonitor.yaml`](../deploy/metrics/servicemonitor.yaml)
scrapes every selected pod through a Service port named `http` and reads the
Bearer token from a Secret. The Prometheus Operator must be installed before
applying that custom resource. Adapt its namespace and selector to the target
deployment, and create the referenced Secret from the same API key supplied to
IronFlow.

[`deploy/metrics/prometheusrule.yaml`](../deploy/metrics/prometheusrule.yaml)
provides starting alerts for run failure ratio, sustained admission rejection,
lease problems, and storage failures. Alert thresholds are examples, not
universal service-level objectives; tune them against the workload and route
them through the deployment's existing notification policy.
