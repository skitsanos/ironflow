# Next IronFlow Release - Draft (1.21.0 candidate)

The approved 1.20.0 changes are recorded in [1.20.0.md](1.20.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

Development continues as `1.21.0-dev.1`. This candidate changes two defaults,
so read the upgrade notes before deploying it over an existing installation.

## Upgrade Notes

- **SQLite is the default state store** (IF-163). A run, `serve` or `list`
  that does not select a backend now opens `ironflow.sqlite` under the store
  directory (`data/runs`, or `/data/runs` in the container), created
  owner-only on Unix. The JSON store is still available and unchanged behind
  `IRONFLOW_STORE=json` or `store_backend: json`, but it is maintenance-only:
  defects are fixed and no new features land in it. There is no automatic
  import between backends. A deployment that relied on the implicit JSON
  default and wants its existing run history must set `IRONFLOW_STORE=json`;
  otherwise it starts with an empty history and the old records stay on disk.
- **Default binaries no longer embed the CJK font** (IF-164). The `cjk-fonts`
  Cargo feature is off by default and on in the `-full` archives and the
  container image. With a default binary, `markdown_to_pdf` on Chinese,
  Japanese or Korean text fails with a hint unless `IRONFLOW_FONT_DIR` points at
  a directory with a CJK font. Default archives shrink from about 106 MB to
  about 48 MiB.
- **Malformed `IRONFLOW_*` limits stop startup** (IF-168). Values such as
  `IRONFLOW_MAX_HTTP_BODY_BYTES=10MB` or a negative limit used to fall back to
  the default silently; `run`, `validate`, `serve` and the engine now reject
  them with the variable name and the accepted form. Documented zeros that
  disable a limit are still accepted.
- **HTML to Markdown conversion moved from `html2md` to `htmd`** (IF-161).
  Output is equivalent for the tested documents, but whitespace and list
  formatting may differ in detail. The shipped binary is now free of GPL
  dependencies and a license gate keeps it so.

## Storage

- SQLite becomes the default state backend; the auto-created store directory is
  `0700` on Unix and a symbolic link in its place is refused, matching the JSON
  store's root rule. The container image needs no new configuration (IF-163).
- The JSON store is frozen as maintenance-only; existing directories keep
  working under the explicit selection (IF-163).

## Build, Binaries And Dependencies

- Release profile strips symbols and links with fat LTO; `pdf-render` is a real
  feature that gates the Pdfium dependency; `cjk-fonts` is opt-in; fonts can be
  loaded at runtime from `IRONFLOW_FONT_DIR` (IF-164).
- Replace GPL-3.0 `html2md` with `htmd`; add `scripts/check_licenses.ts`, a
  copyleft gate over the shipped dependency graph that runs in CI, the local
  gate and the pre-commit hook (IF-161).
- Pin every GitHub Action to a commit SHA with Renovate digest pinning, grant CI
  read-only tokens, deny stray `unwrap`/`expect` in production code, and drop
  the `num_cpus` dependency in favor of the standard library (IF-171).
- Take 57 compatible dependency updates (hyper 1.12, h2 0.4.20, aws-lc-rs 1.19,
  ICU 2.4, tokio-rustls 0.26.6 and others); `cargo audit` and the license gate
  stay clean. The `markup5ever_rcdom` 0.39 update is deferred until `htmd`
  moves to html5ever 0.39 or later.

## Reliability And Observability

- Malformed `IRONFLOW_*` environment limits fail at startup for every entry
  point instead of silently defaulting (IF-168).
- The executor's run, task-attempt, finalization and lease-heartbeat paths, the
  scheduler decision, the Lua Flow API and the DOCX block parser are
  restructured into smaller units; behavior is unchanged and covered by the
  existing suites (IF-165).

## Development Process

- Integration tests build as a bounded set of area binaries instead of one
  executable per file, removing the disk and CI compensation that layout
  required (IF-162).
- CI runs the feature job only on the binaries with feature-gated tests, macOS
  pull requests run the platform-sensitive areas, and the release build runs on
  pushes only (IF-166).
- A `develop` push requires a receipt from a passing `scripts/integration_gate.sh`
  run on the pushed tree; the gate no longer runs inside the push's SSH session,
  which GitHub closes before it finishes. `IRONFLOW_PREPUSH_GATE=fast` remains
  for intermediate pushes (IF-167, IF-172).
- The module-size gate scans production files only and treats shrinking as a
  warning with `--ratchet`; Clippy bounds functions at 100 lines and complexity
  25 (IF-165). Agent skills have one canonical source under `.agents/skills`;
  policy tests that only restated configuration text were removed (IF-169).
- Documentation drift fixes: node counts are checked against the registry, and
  stale references to the previous defaults were corrected (IF-170).

## Dependencies And Security (carried from the 1.20.1 draft)

- Update AWS S3 to `1.156.0`, S3 Vectors to `1.46.0`, and the shared Smithy
  runtime cohort, including `aws-sigv4 1.6.4`.
- Restore SigV4a for supported S3 Multi-Region Access Point object operations
  and presigned URLs. Require S3 SDK `>=1.156.0`, signer `>=1.6.4`, and patched
  `crypto-bigint 0.7.10`; preserve regional SigV4 and fail-closed audit policy.
  See [security guidance](../SECURITY.md#s3-signing) for limits and the
  [MRAP example](../../examples/04-file-operations/s3_mrap_presign.lua).
- Update ZIP to `9.1.3`, including AES key zeroization, ZIP64 offset and
  archive metadata fixes, and Windows symlink-safe extraction improvements.

## Container Publication

- Publish new container builds to Docker Hub alongside GHCR and promote
  verified images to stable version tags after successful releases. See
  [Docker publishing](../DOCKER_PUBLISHING.md) for configuration and limits.
- The pre-publication store smoke now asserts the default SQLite database and
  the absence of JSON records under both runtime UIDs.

## Downloads And Compatibility

- Four platforms, each with default and `-full` archives: Linux x86-64 (static
  musl), macOS Apple Silicon, macOS Intel and Windows x86-64.
- Default archives use the SQLite store by default and keep JSON selectable;
  `-full` adds PostgreSQL and Redis backends and the embedded CJK font. The
  container image includes all of them. No database servers or native Pdfium
  are bundled.
- `-full` archives and the container image include the fonts' OFL license.
  Verify downloads against the accompanying `checksums-sha256.txt` manifest.

Earlier maintenance changes remain in [the 1.19.2 notes](1.19.2.md).
