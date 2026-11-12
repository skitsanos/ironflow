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
floor. `cargo audit --deny warnings` remains mandatory; its only advisory
ignore is the reviewed SQLite entry under
[Dependency advisories](#dependency-advisories).
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

## Outbound destinations

A flow chooses where its nodes connect. Only the `http_request`, `http_get`,
`http_post`, `http_put` and `http_delete` nodes can restrict that choice:
`block_private_network = true` refuses `localhost` and every address that is
not a public unicast destination, for the initial URL, every redirect target
and every DNS answer, and pins the validated addresses to the connection. It
is off by default.

Refused IPv4 ranges: `0.0.0.0/8`, `10.0.0.0/8`, `100.64.0.0/10`,
`127.0.0.0/8`, `169.254.0.0/16`, `172.16.0.0/12`, `192.0.0.0/24`,
`192.0.2.0/24`, `192.88.99.0/24`, `192.168.0.0/16`, `198.18.0.0/15`,
`198.51.100.0/24`, `203.0.113.0/24`, `224.0.0.0/4` and `240.0.0.0/4`.

Refused IPv6 ranges: everything outside `2000::/3` (including `::`, `::1`,
IPv4-compatible and IPv4-translated forms, `64:ff9b:1::/48`, `100::/64`,
`5f00::/16`, `fc00::/7`, `fe80::/10`, `fec0::/10` and `ff00::/8`), plus
`2001::/23`, `2001:db8::/32` and `3fff::/20`. IPv4-mapped (`::ffff:0:0/96`),
NAT64 (`64:ff9b::/96`) and 6to4 (`2002::/16`) addresses are refused when the
IPv4 address they embed is in a refused IPv4 range and allowed otherwise.
The node reference describes each range under
[Private-network blocking](nodes/http_request.md#private-network-blocking).

No other node has this option or an equivalent address restriction:

| Consumer | Destination setting | Restriction present |
|----------|---------------------|---------------------|
| `mcp_client` (Streamable HTTP) | `url` | None on the address. Only same-origin `307`/`308` redirects are followed. |
| `llm`, `ai_embed`, `ai_chunk_semantic`, `transcribe` | `base_url`, `azure_endpoint`, `ollama_host`, OAuth `token_url`, and their environment variables | None on the address. Redirects are same-origin only. |
| `slack_notification`, `send_email` with `resend` | `webhook_url`, `api_url` | None on the address. Redirects are same-origin only. |
| `send_email` with `smtp` | `smtp_server`, `smtp_port` | None. |
| `arangodb_aql` | `url` | Scheme must be `http` or `https`; none on the address. Redirects are same-origin only. |
| `db_query`, `db_execute` | `connection` | None. |
| `s3_*` and S3 Vectors nodes | `endpoint_url`, `AWS_ENDPOINT_URL` | None. |
| `shell_command`, `mcp_client` (stdio) | the command | None; a subprocess can open any connection. |

These nodes also use the system proxy when one is configured. Treat a flow
that sets any of these values from untrusted input as able to reach the
internal network, and confine the IronFlow process with network policy
(egress rules, a metadata-service hop limit or block) where that matters.
The artifact store endpoint and the state-store URLs are operator
configuration and cannot be changed by a flow.

## Forwarded webhook headers

A webhook passes a request header to a flow only when `forward_headers` names
it. The value is an invocation-only input: it is merged into `ctx._headers`
for each step and child workflow and is never written to the stored context,
task history, events or run-detail responses (IF-011).

Redaction contract (IF-176):

- Secrets are the complete header value and each component of at least eight
  non-whitespace bytes: the credential after `Bearer` or `Basic`, the value of
  each `key=value` pair split on `,` or `;`, and each string or number inside
  a value that parses as JSON. The JSON-escaped form is matched too.
- Header names, JSON object keys, booleans, `null` and shorter components are
  not secrets. A webhook rejects a forwarded value below the same eight-byte
  floor with `400`, so every accepted value is matched wherever it occurs.
- In step output that later steps read, a secret inside a string becomes
  `[REDACTED]` and an object key containing one is removed. Numbers keep their
  JSON type and top-level `_route_*` and `_condition_result_*` keys are never
  dropped or rewritten, so a header cannot reroute a run or change the shape
  of live data.
- Records are stricter: task history, the stored context, events and
  `GET /runs/{id}` also replace a number equal to a secret with `[REDACTED]`
  and redact routing keys like any other field.

Boundary. Matching is literal: an encoded, hashed or split copy of a value is
not recognized, so flows must validate a forwarded value in place and not
transform or send it elsewhere. Redaction also acts on text the sender chose.
A sender who can set a forwarded header can blank a string of eight or more
bytes in later step output, which can change a branch that compares that
string. Forward headers only from senders that the webhook `signature` policy
or an upstream gateway authenticates, and keep non-confidential metadata in
the JSON body. The full table is in the
[CLI reference](CLI_REFERENCE.md#forwarded-header-redaction).

## Diagnostic scrubbing

Error text can carry a credential that no configuration names: a provider
echoes a request header, a driver prints its connection string, a script
raises an error that quotes an environment variable. A pattern scrubber
therefore runs on the error of every failed task attempt before it is logged,
stored or published, on failure summaries and panic messages, and on
scheduler, storage, API and CLI diagnostics. Nodes that quote a provider or
database response in an error scrub it as well. In the engine the scrubber
runs after the exact-value redaction described above
([IF-177](issues/IF-177.md)).

Guaranteed shapes:

| Shape | Example | What is replaced |
| --- | --- | --- |
| URL with the scheme `http`, `https`, `postgres`, `postgresql`, `redis`, `rediss`, `smtp`, `smtps` or `sqlite` | `postgres://app:pw@db/app?sslmode=require` | User information, every query value and the fragment; for `http` and `https` the path too. A URL that does not parse becomes `[REDACTED URL]`. |
| Assignment with `=` | `db_password=pw`, `--token=pw`, `PGPASSWORD=pw` | The value up to the next whitespace or `&`, `;`, `,`, `<`, `>` or quote. |
| `Bearer` or `Basic` after `=` | `Authorization=Bearer pw` | The scheme and the credential after it, as one value. |
| Assignment with `:` | `X-API-Key: pw`, `Proxy-Authorization: Basic pw` | The value up to the next `&`, `;`, `,`, `<`, `>`, quote or line end. |
| Quoted value | `"client_secret": "pw"`, `password = 'p''w'` | The content between the quotes. A backslash escape or a doubled quote does not end it. |
| JSON inside a JSON string | `{\"password\":\"pw\"}` | The content between the escaped quotes, at any escaping depth. |

A key is sensitive when it ends, in any letter case, with `access_key`,
`api_key`, `authorization`, `credential`, `credentials`, `passphrase`,
`passwd`, `password`, `private_key`, `pwd`, `secret`, `secret_key`,
`signature` or `token`. An underscore in those endings also matches a hyphen
or nothing, so `X-API-Key`, `apiKey` and `aws_secret_access_key` are covered.
Matching by ending covers compound names without a list of them and leaves
`token_type`, `max_tokens`, `password_policy` and `secret_name` readable.

Every replaced value becomes `[REDACTED]`, and control characters become
spaces so a diagnostic stays on one log line. Only the first 64 KiB of a
diagnostic is scanned and kept; the rest is replaced by `...[truncated]`, and
a URL that the limit cuts becomes `[REDACTED URL]`. The scan reads the text a
fixed number of times, so its cost grows linearly with the input, and the
engine redacts a task error above the limit on the blocking pool.

Boundary. The scrubber is a last line of defence that recognises shapes, not
secrets. It does not recognise:

- a credential with no sensitive key or supported URL around it, including a
  bare `Bearer pw` in prose;
- plural keys other than `credentials` (`tokens`, `passwords`, `secrets`) and
  keys whose sensitive word is not last;
- more than the first word of an unquoted value after `=`, except for the two
  schemes above;
- the operand of a comparison: `password == pw` is not an assignment, and a
  sensitive key followed by `==` is left as written so an error that quotes
  a guard condition stays readable (IF-187). A value that starts with `=`,
  as in `password==pw`, is still replaced;
- a value that continues on another line, such as an unescaped PEM block;
- a connection string without a scheme, outside IronFlow's own Redis and SQL
  connection errors;
- an encoded, hashed or split copy of a credential.

Keep credentials out of error messages that a flow raises itself, and rely on
forwarded-header and node credential redaction for values IronFlow knows.

## Dependency advisories

`cargo audit --deny warnings` fails on every vulnerability and warning that is
not listed in `.cargo/audit.toml`. Three advisories published after the 1.21.0
release were handled on 2026-11-12 ([IF-173](issues/IF-173.md)):

| Advisory | Crate | Outcome |
| --- | --- | --- |
| [RUSTSEC-2026-0462](https://rustsec.org/advisories/RUSTSEC-2026-0462.html) | `rmcp` | Updated to 3.9.0 and required as the minimum. `mcp_client` over Streamable HTTP was affected: JSON and error bodies were read without a limit. They are now capped at 16 MiB and 64 KiB. |
| [RUSTSEC-2026-0436](https://rustsec.org/advisories/RUSTSEC-2026-0436.html) | `calamine` | Updated to 0.38.0. The defect is in the `.xls`/`.xlsb`/VBA container parser, which `extract_xlsx` never calls. |
| [RUSTSEC-2026-0459](https://rustsec.org/advisories/RUSTSEC-2026-0459.html) | `libsqlite3-sys` | Not fixable yet; one reviewed exception, described below. |

Updating `calamine` was not free of risk. Version 0.38 builds a style record
for every cell format when a workbook opens and copies the font name and
number-format code into each one, so a workbook of a few kilobytes could hold
gigabytes before a cell was read. `extract_xlsx` now bounds that table against
`IRONFLOW_MAX_XLSX_OUTPUT_BYTES` before the workbook is opened; see
[`extract_xlsx`](nodes/extract_xlsx.md#ceilings).

### SQLite exception (RUSTSEC-2026-0459)

The bundled SQLite is 3.51.3. Before 3.54.0, SQLite can return uninitialized
page-cache memory when a database is opened together with a crafted sibling
`-wal` file (CVE-2026-119375). The fix ships in `libsqlite3-sys` 0.39.0, but
the newest published `sqlx-sqlite` (0.9.0) requires a version below 0.38, so
no fixed version resolves. The `sqlx` development branch already accepts it.

- **Exposure.** An attacker must write both the database file and its `-wal`
  file, and IronFlow must then open them. The state and event stores live in
  an owner-only directory (IF-163) and are not exposed by this. The exposed
  path is a `db_query` or `db_exec` flow whose `connection` names a SQLite
  file that a less trusted party could have written: its rows could carry
  stale memory of the IronFlow process into workflow context.
- **Until it is fixed.** Do not point `db_query` or `db_exec` at SQLite files,
  or at directories, that an untrusted party can write. PostgreSQL connections
  are not affected.
- **Exit.** The entry is removed when a published `sqlx` release resolves
  `libsqlite3-sys` 0.39.0 or later. A repository policy test fails as soon as
  the lockfile holds a fixed version while the entry remains, and whenever a
  second entry is added. The exception is reviewed again by 2026-12-12 if no
  such release exists.

### Scheduled audit

Advisories are published against code that is already released, so the CI
`audit` job (the dependency audit and the license check) also runs every day
at 05:23 UTC, without a push. A scheduled run uses the workflow and lockfile
of the default branch, `main`, and runs no other job. It therefore covers the
released branch only once this workflow has reached `main`, and `develop` is
still audited on pushes and pull requests. GitHub disables scheduled workflows
in a public repository after 60 days without repository activity; re-enable
the workflow if that happens.

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

Attribution for the same graph ships as `THIRD_PARTY_LICENSES.md` in every
release archive and at `/usr/local/share/licenses/ironflow/` in the container
image (IF-204). `scripts/third_party_notices.ts write` renders it from the
gate's shipped-package list: one row per crate, without a version, with its
SPDX expression, the license IronFlow uses it under (the first allowed option in
the preference order recorded in the script when a crate offers a choice) and
its repository link, an MPL-2.0 section that states where the unmodified
source is available, and each applicable canonical license text once from
`scripts/licenses/`. The file is committed; `third_party_notices.ts check`
runs next to the license gate in the CI `audit` job, the integration gate and
the pre-commit hook. Because rows carry no version (IF-208), a version-only
lockfile update leaves the file current, so a Renovate pull request for one
passes unaided; the exact versions are those in the `Cargo.lock` of the
revision a binary was built from. The check fails when a crate is added or
removed or a license or source changes, until the file is regenerated:

```sh
bun --no-env-file scripts/third_party_notices.ts write
```

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
