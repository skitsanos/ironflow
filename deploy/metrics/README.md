# Prometheus Operator templates

These manifests are production starting points for an IronFlow deployment in
the `ironflow` namespace. They assume:

- `IRONFLOW_METRICS_ENABLED=true` is set on every IronFlow pod;
- the Service selects those pods and names port 3000 `http`;
- `IRONFLOW_API_KEY` comes from Secret `ironflow-metrics-auth`, key `api-key`;
- Prometheus Operator CRDs are installed; and
- the Prometheus instance selects resources labeled `release: prometheus`.

Change the namespace, labels, and selectors to match the cluster. Create the
Secret through the platform secret manager; do not add its value to these
files. Because IronFlow currently uses one API credential, the token granted to
Prometheus can authenticate to every protected endpoint. Restrict network
access from the monitoring namespace to the IronFlow pods.

Apply the templates after adapting them:

```bash
kubectl apply -f deploy/metrics/servicemonitor.yaml
kubectl apply -f deploy/metrics/prometheusrule.yaml
```

See [`docs/METRICS.md`](../../docs/METRICS.md) for metric semantics, reset
behavior, aggregation guidance, and the complete bounded label contract.
