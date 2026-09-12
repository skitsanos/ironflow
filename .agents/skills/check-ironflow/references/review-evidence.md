# Fresh Review Evidence

Record branch, commit, dirty changes, dependency/toolchain versions, and scope
before inspecting a fresh candidate. Passing existing gates establishes a
baseline, not the absence of defects. Keep review output separate from fixes.

Prioritize security, silent corruption, availability, and durable execution.
Trace shared contracts through all callers before generalizing a finding:

- Provider HTTP: custom auth headers and private bodies on redirects, exact
  environment-only credentials in errors, response admission, and cancellation.
- Files/documents: every representation the dependency can open, missing-path
  branches, symlink targets, archive relationships, inherited page data, and
  parser/resampler allocations that happen before IronFlow's own limits.
- Composition: live versus persisted context, missing paths, metadata collisions,
  literal versus expression mapping, and validate/run parity.
- Storage/protocols: preflight before partial mutations, concurrent interleavings,
  backend-label versus actual-dialect checks, and partially consumed cancellable IO.

Use bounded synthetic fixtures and credentials. Prefer public Node/CLI/API
regressions with a positive control. Label static reasoning, query-level tests,
production-script execution, mocked protocols, and live service evidence
separately. Do not use real secrets or shared stores to prove a defect.

Each retained finding needs a concrete trigger, observed versus expected result,
source location, severity rationale, contract boundary, and acceptance test.
Keep a demonstrated defect separate from a robustness/design hypothesis. Do not
claim all current defects were introduced by the candidate's dependency upgrade.

When registration is requested, allocate the next unused sequential IF IDs and
write canonical issue pages before code changes. Keep a dated audit snapshot
and finding-to-issue map; generated indexes are not manual issue records.
Turn temporary probes into portable regressions during remediation. Mark an
issue resolved only after the actual fix's tests and surface checks pass.

Delegate only when the user or existing instructions authorize it. Give each
worker a distinct scope and evidence requirements; reconcile unfinished worker
results explicitly. A worker's review conclusion does not replace parent
verification or authorize fixes, commits, pushes, or publication.
