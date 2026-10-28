# TLS trust-store maintenance

## eMudhra incident: 2026-10-24 assessment

The October 20 [intermediate-key disclosure](https://emudhra.we-have-a-problem.com/)
affects clients that still accept the compromised certificate chains. Updating
a browser or host alone does not update bundled roots inside an existing
IronFlow binary. An affected dependency graph is not proof of interception.

The changes prepared for v1.19.1 address this with `rustls-webpki >= 0.103.16`,
`webpki-roots >= 1.0.10`, and `webpki-root-certs >= 1.0.10`. Both Docker stages
require Debian bookworm `ca-certificates >= 20250419~deb12u2`. CI requires a
patched Bun runtime (>= 1.4.4). See [IF-151](issues/IF-151.md) for the exact
validation state; confirm publication against the tagged release and do not
infer a production rollout from these source changes.

The pre-fix published release tested during this assessment was v1.19.0. Both
curl and an IronFlow `http_get` workflow in its then-current `latest` image
accepted a compromised chain in local reproductions; its immutable
digest is recorded in IF-151. Do not treat
that release or an already-running image as protected by this source update.

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

For PostgreSQL, use `sslmode=verify-full` with the appropriate CA. Encryption-
only `sslmode=require` does not verify the server certificate and cannot
provide this identity protection. `verify-ca` verifies the chain but not the
hostname. Existing configuration is not silently upgraded to a stronger mode.

## S3 signing

IronFlow uses AWS Signature Version 4 (SigV4) for regional buckets and enables
SigV4a for supported S3 Multi-Region Access Point (MRAP) object operations.
The SDK selects the signing scheme from the endpoint; enabling SigV4a does
not switch ordinary regional requests to asymmetric signing. Regional
S3-compatible endpoints and S3 Vectors retain their existing behavior.

For MRAPs, supply the ARN, for example
`arn:aws:s3::123456789012:accesspoint/fixture.mrap`, as the node's `bucket` or
`IRONFLOW_ARTIFACT_S3_BUCKET`. An alias or hostname is not a substitute for the
ARN. Leave custom endpoints unset and keep `force_path_style` /
`IRONFLOW_ARTIFACT_S3_FORCE_PATH_STYLE` false. The normal credential and region
provider chains still apply. `AWS_SIGV4A_SIGNING_REGION_SET` can narrow the
signing region set; the MRAP default is `*`.

[AWS's operation restrictions](https://docs.aws.amazon.com/AmazonS3/latest/userguide/MrapOperations.html)
still apply: copy supports MRAPs only as a destination, and `s3_list_buckets`
does not enumerate or manage MRAPs. This restoration does not create access
points, configure replication/routing, or grant IAM permissions. Treat
presigned URLs as bearer credentials and forward any returned required headers.

For shared artifacts, MRAP signing is not a replacement for replication or
read-after-write visibility across Regions. A read can reach a bucket where
the object has not replicated and return 404. Prefer one regional bucket when
immediate cross-replica reads are required; qualify MRAP replication, routing
and pruning separately. See
[AWS's replication guidance](https://docs.aws.amazon.com/AmazonS3/latest/userguide/MultiRegionAccessPointBucketReplication.html).

### 2026-10-28 restoration

SigV4a was disabled on 2026-10-24 to remove the vulnerable bigint dependency
([RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html)).
The initial 2026-10-28 dependency refresh kept that mitigation while the
upstream fix was assessed. With approval, development restores the feature
and raises the minimum S3 SDK to `1.156.0`, requiring `aws-sigv4 >= 1.6.4`.
The
[upstream release](https://github.com/awslabs/aws-sdk-rust/releases/tag/release-2026-10-28)
requires `crypto-bigint >= 0.7.10`; the resolved graph contains only `0.7.10`,
including with both storage features enabled. Policy tests guard the SDK and
signer floors and every resolved bigint copy. No audit exception or local
crypto patch is used. The bigint timing advisory is separate from the eMudhra
certificate-trust incident.

Offline regressions cover MRAP header signing, GET/PUT/HEAD/DELETE presigning,
session tokens, signing region sets, regional SigV4 preservation and invalid
path-style rejection. They do not establish live AWS permissions, routing,
replication or failover acceptance. Published older binaries keep the signing
behavior of their tagged source; source changes do not upgrade deployments.

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

## Dependency licenses

IronFlow is MIT licensed and ships statically linked binaries and container
images, so every crate that is compiled into them must carry a permissive
license. `scripts/check_licenses.ts` resolves the dependency graph with
`cargo metadata --locked --all-features`, follows normal and build dependencies
from the root (dev-dependencies never ship), and fails closed on any package
whose SPDX expression is missing, unparsable, or not satisfiable from the
reviewed allowlist in that script. `OR` alternatives pass when one option is
allowed; `AND` operands must all be allowed. A package that only carries a
license file needs a `name@version` entry in `LICENSE_EXCEPTIONS` with the
reviewed finding, and the entry becomes a violation once that version leaves the
graph.

The check runs in the CI `audit` job, in `scripts/integration_gate.sh`, and in
the pre-commit hook whenever `Cargo.toml`, `Cargo.lock`, or the script itself is
staged:

```sh
bun --no-env-file scripts/check_licenses.ts
```

The first run of this gate found `html2md` (GPL-3.0+) in the shipped graph; it
was replaced by `htmd` (Apache-2.0) in IF-161. Fonts embedded for PDF generation
remain under the SIL Open Font License shipped as `assets/fonts/OFL.txt`.

## Watched dependencies

`noyalib` parses `IronFlowConfig` YAML and backs the `yaml_parse` /
`yaml_stringify` nodes. It is a young crate (0.0.x versions, a single
maintainer) whose nesting depth IronFlow already bounds (IF-037). It stays on a
watch list with an exit criterion: migrate to a maintained YAML parser if a
release changes its public API incompatibly, if six months pass without a
release, or if an advisory is published against it. The dependency audit in
CI (`cargo audit --deny warnings`) and the Renovate cohort both surface those
events.

GitHub Actions in every workflow are pinned to commit SHAs with the version
noted in a comment; Renovate's `helpers:pinGitHubActionDigests` preset keeps
the digests current. The only exception is `dtolnay/rust-toolchain`, whose tag
is the Rust version that `rust-toolchain.toml` and the Docker builder image
must match, so the toolchain update group pins it by version instead.
