# TLS trust-store maintenance

## eMudhra incident: 2026-10-24 assessment

The October 20 [intermediate-key disclosure](https://emudhra.we-have-a-problem.com/)
affects clients that still accept the compromised certificate chains. Updating
a browser or host alone does not update bundled roots inside an existing
IronFlow binary. An affected dependency graph is not proof of interception.

The development candidate addresses this with `rustls-webpki >= 0.103.16`,
`webpki-roots >= 1.0.10`, and `webpki-root-certs >= 1.0.10`. Both Docker stages
require Debian bookworm `ca-certificates >= 20250419~deb12u2`. CI requires a
patched Bun runtime (>= 1.4.4). See [IF-151](issues/IF-151.md) for the exact
validation state; do not infer a published release or production rollout from
these development changes.

## Trust sources

| Surface | Trust source and update responsibility |
| --- | --- |
| HTTP nodes and provider clients | reqwest/rustls platform verification; keep native trust and the compiled dependency graph current. |
| AWS/S3, SMTP, PostgreSQL and Redis TLS | rustls verification with native, bundled, or configured roots, depending on the client; update the graph and environment. |
| Operator CA files | Administrator-managed inputs, not automatically replaced by dependency updates. Never disable verification to restore connectivity. |
| Container build and runtime | Independently installed Debian CA packages; rebuild with fresh package indexes and current base images. |
| Website and Bun scripts | Bun's embedded trust store; update Bun and rebuild/redeploy existing images. Floating tags do not update running containers. |

Repository policy tests guard the resolved Rust versions and container package
floor. `cargo audit --deny warnings` remains mandatory without advisory ignores.
No vendored PEM certificate bundles were found in the tracked application,
test, script, documentation, or example trees during this assessment.

## S3 signing

IronFlow uses regional AWS Signature Version 4 (SigV4). SigV4a is disabled
to avoid the AWS SDK's affected optional `crypto-bigint 0.5` dependency
(RUSTSEC-2026-0387). S3 Multi-Region Access Points, which require SigV4a, are
not supported. Use regional buckets or regional access points instead. This
applies to every S3 node and the S3 artifact backend. Regional S3, compatible
regional endpoints, presigned URLs, and S3 Vectors remain available. There is
no configuration or feature switch that re-enables the affected signer.

## Runtime checks and rollout

Build the candidate, then run:

```sh
bun --no-env-file scripts/test_public_tls.ts target/release/ironflow
```

This opt-in network check requires a valid HTTPS control to succeed and four
disclosed direct/cross-signed, CT/no-CT probes to fail with certificate errors.
A timeout, DNS failure, or refused connection is inconclusive and fails the
check. It sends no application credentials, inherits no proxy settings, and
does not modify host trust. Set `IRONFLOW_TEST_PUBLIC_TLS=1` to enable it in
the full integration gate; ordinary CI does not depend on public test hosts.

Rebuild and redeploy affected images, replace old binaries, and update native
trust stores and administrator-managed bundles. Re-run checks from the actual
deployment environment. Local results do not establish protection on other
machines or old deployments. Review historic endpoint, proxy, and TLS logs
separately; a patched audit cannot establish absence of past exploitation.
