# Next IronFlow Release - Draft

The approved 1.21.0 changes are recorded in [1.21.0.md](1.21.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

## Upgrade Notes

- **Schedules written with numeric weekdays move by one day** (IF-180). The
  weekday field of a schedule's `cron` now uses the standard crontab
  numbering: `0` and `7` are Sunday, `1` to `6` are Monday to Saturday.
  Earlier releases read the numbers one day early (`1` as Sunday, `7` as
  Saturday) and rejected `0` at startup. After the upgrade `0 9 * * 1-5` runs
  Monday to Friday instead of Sunday to Thursday, and `0 0 * * 7` runs on
  Sunday instead of Saturday. Review every schedule whose fifth field
  contains a digit before deploying: one that was written for standard cron
  now fires on the intended days, and one whose numbers were adjusted to
  compensate must be changed back or rewritten with names. Schedules that use
  weekday names (`Mon-Fri`), `*` or `*/N` are unaffected. A weekday outside
  `0` to `7` or a backwards range such as `6-1` still fails at startup, now
  with a message that names the weekday field and states the numbering.
  Stray characters after a weekday, which the parser could read as a year
  (`Mon2030`), are rejected as well. See the weekday table under
  [Schedules](../CLI_REFERENCE.md#weekday-field).
- **A schedule whose `cron` has a malformed field now stops `serve` at
  startup** (IF-206). Each of the five fields must be one token. Earlier
  releases passed the fields to a parser that separates them with optional
  whitespace and accepts a trailing year, so a token with a missing space was
  read as two fields and every later field shifted: `5*/2 * * * *` ran at
  minute 5 of every second hour, and `0 2 * 1* *` ran at 02:00 daily in
  January only. A token that starts or ends with `-`, `,` or `/` could join
  the field beside it in the same way. These expressions are now rejected
  with a message that names the field, for example `minute '5*/2' is not a
  single field`. Well-formed expressions are accepted as before and fire at
  the same times. Check every schedule in `ironflow.yaml` before deploying:
  one that loaded with a mistyped field has been running at the wrong times,
  and it will now fail startup until it is corrected. See
  [Field syntax](../CLI_REFERENCE.md#field-syntax).
- **Files written by staged-writer nodes are no longer owner-only on Unix**
  (IF-185). `write_file`, `base64_decode` with `output_file`, `zip_create`,
  `zip_extract`, `pdf_merge`, grouped `pdf_split`, `markdown_to_pdf` and
  `markdown_to_docx` published every file as mode `0600`, whatever the umask
  and whatever mode the replaced file had. A new file now gets `0666` masked
  by the process umask (`0644` under umask `022`), and an overwritten or
  appended file keeps its permission bits, so an appended `0755` script stays
  executable. Setuid, setgid and sticky bits are not carried, `zip_extract`
  ignores permission metadata stored in the archive, and `copy_file` still
  publishes the source mode. A deployment that relied on these outputs being
  private to the IronFlow user must now run IronFlow with a restrictive umask
  such as `077`, or write into a directory other users cannot enter.
- **Parallel `write_file` appends no longer lose lines** (IF-185). Eight
  parallel steps appending to one file could all succeed and leave fewer
  lines, because each append rewrote the file from the content it had read.
  Writes to one destination are now serialized inside the IronFlow process.
  Appends to one file from several IronFlow processes are still
  last-writer-wins; give each process its own file.
- **Deeply nested child workflows and oversized fan-outs now fail** (IF-188).
  A child run more than 16 levels below its top-level run is refused, counted
  across `subworkflow` (waiting or detached), `parallel_subworkflows`,
  `repeat_subworkflow` and `tool_dispatch`. A flow that called itself used to
  descend until the process ran out of memory; its step now fails with
  `child workflow nesting depth 17 exceeds the limit of 16
  (IRONFLOW_MAX_SUBWORKFLOW_DEPTH)`. One `parallel_subworkflows` call accepts
  at most 1024 entries and fails before starting any child when given more.
  Both refusals are step errors that `on_error = "ignore"` on the same step
  does not suppress. A workflow that legitimately nests deeper or fans out
  wider must set `IRONFLOW_MAX_SUBWORKFLOW_DEPTH` or
  `IRONFLOW_MAX_PARALLEL_SUBWORKFLOWS`; both are validated at startup. See
  [Nesting limit and waiting](../nodes/subworkflow.md#nesting-limit-and-waiting)
  and [Fan-out bounds](../nodes/parallel_subworkflows.md#fan-out-bounds).
- **A timed-out or cancelled step waits for its child workflows to stop**
  (IF-188). When a step that waits for a child run timed out, its `on_error`
  handler started and the parent run could be finalized `success` while a
  tracked blocking worker in the child was still running. The step now ends
  only after the child run has stopped, so a handler that cleans up or retries
  no longer races the work it replaces, and run admission is not released
  early. A child whose worker cannot be interrupted delays the timeout result
  by the time that worker needs to return, as the same node placed directly
  in the parent already did. `parallel_subworkflows` also builds each child's
  context only when the child starts; a flow path that cannot be resolved
  still fails the step before any child runs.
- **New connections are answered while `serve` drains, and exit is bounded**
  (IF-183). After SIGTERM or SIGINT a new connection completed the TCP
  handshake and then received nothing until the drain ended, up to
  `IRONFLOW_SHUTDOWN_GRACE_SECONDS` plus the settle window, so a readiness
  probe timed out instead of seeing the documented `503`. The listener now
  stays open during the drain: `/health/ready` answers `503` with
  `"status": "draining"`, `POST /flows/run` and webhooks answer `503`, and
  run status and events remain readable. The listener closes when accepted
  runs have drained. The process then waits at most five seconds for open
  connections and five more for blocking workers that cannot be interrupted,
  where it previously waited for those workers without limit; this also
  applies to `ironflow run`, which exits at most five seconds after printing
  its result. The longest termination is now
  `IRONFLOW_SHUTDOWN_GRACE_SECONDS + 30` seconds (documented as `+ 25`
  before): raise a platform termination window that was set to the old
  figure. See [Replica deployment](../REPLICA_DEPLOYMENT.md).

- **`write_file` artifact inputs no longer accept a local path** (IF-186).
  `artifact = "/some/file"`, `content` with `encoding = "artifact"`, and a
  `source_key` value read with `encoding = "artifact"` were documented as an
  artifact descriptor or canonical `artifact://sha256/...` URI, but any other
  string was opened as a local file and copied to the destination. Such a
  step now fails with `write_file: artifact input must be an artifact
  descriptor or a canonical artifact://sha256/... URI`, as the `http_*`
  artifact body already did. Replace a path input with `copy_file`, or store
  the file first with `read_file` and `encoding = "artifact"`. Descriptors
  and canonical URIs are copied as before.
- **Source paths are interpolated once, and context values are not
  templates** (IF-186). `path` on the `extract_*` nodes, `extract_srt`,
  `extract_vtt`, `transcribe`, `pdf_metadata`, `pdf_split`, `pdf_to_image`
  and `pdf_thumbnail` was expanded twice: a `${ctx...}` placeholder inside
  the resolved context value was expanded again, and the `\${ctx...}` escape
  produced the expanded value instead of the literal text. The same nodes,
  the image nodes, `image_to_pdf`, `pdf_merge` and `write_file` also expanded
  placeholders in a value read through `source_key`; `s3vector_put_vectors`
  and `s3vector_delete_vectors` did so for keys read through
  `vectors_source_key` and `keys_source_key`, and `slack_notification`
  expanded `payload.text` twice. All of these now resolve configuration
  strings once and use context values as stored, as the
  [Lua flow guide](../LUA_FLOW_GUIDE.md#context-variable-interpolation)
  describes. A flow that relied on the second pass, for example a context
  value holding `"${ctx.dir}/report.pdf"` that a later node opened through
  `source_key`, must build the final path first with `template_render` or a
  function step.

- **Context and step output nested deeper than 123 levels are refused**
  (IF-181). A value nested 126 levels deep passed the JSON parser, and the
  JSON and Redis stores then wrote a run record they could not read back:
  the run stayed `pending`, `ironflow inspect` failed, and once its lease
  expired `ironflow serve` could not start on a JSON store and Redis stopped
  reconciling every later expired lease. A step output nested 127 levels
  deep did the same to a SQLite or PostgreSQL run. One fixed limit now
  applies everywhere: a context value or step-output value may nest arrays
  and objects at most 123 levels. `ironflow run --context`, `POST /flows/run`
  and webhook bodies refuse a deeper value before a run exists (exit status
  1 or `400`), and a step that returns one fails without being retried.
  Values between 124 and 126 levels that the SQL stores accepted before are
  now refused as well. Lease reconciliation skips a record it cannot decode
  and logs a warning that names the run; remove such a run with
  `DELETE /runs/{id}`. See
  [JSON nesting limit](../CLI_REFERENCE.md#json-nesting-limit).

- **`step_if` and `if_node` compare with `true`, `false` and `nil`, and
  undefined comparisons fail** (IF-187). A comparison whose context value
  was a boolean, `null`, an object or an array returned false for every
  operator, so `ctx.ready == true`, `ctx.ready != false`, `ctx.off == false`
  and `ctx.missing == nil` skipped their step in a run that finished
  `success`. `==` and `!=` now accept `true`, `false` and `nil`, where `nil`
  matches a key that is absent or `null`, and `!=` is the exact negation of
  `==`. Numeric and string comparisons, `exists`, truthiness and a string
  compared with an unquoted operand are unchanged. Review guards for the
  results that change:
  - `ctx.missing != nil` is now false (it was true), and `ctx.missing == nil`
    is true.
  - `!=` between values of different types is now true where it was false:
    a number against a non-numeric operand (`ctx.n != 'abc'`), a `null`
    value against anything except `nil`, and a boolean against anything
    except the same boolean.
  - An ordering operator (`>`, `<`, `>=`, `<=`) whose right operand is not a
    number, such as `ctx.n > nil` or `ctx.a > ctx.b`, was always false and
    now fails flow loading with a message that names the step and condition.
  - Ordering a context value that is not a number was always false and now
    fails the guard step and the run with an error that names the condition.
    This includes a missing or `null` key and a numeric string: a guard such
    as `ctx.retries > 3` used to skip its step while `retries` was unset and
    now fails, so give the key a default in an upstream step or in the run
    context. Comparing an object or array with anything other than `nil`
    fails in the same way.
  - A condition is split at its first operator. `ctx.s != 'a==b'` used to
    split at the `==` inside the string and route false; it now compares
    `s` with the text `a==b`.
  - A condition outside the grammar was read as a missing path or as one
    long string, routed false, and now fails flow loading: `not ctx.ready`,
    `!ctx.ready`, `ctx.a and ctx.b`, `ctx.n == 1 and ctx.s == 'x'`,
    `(ctx.n > 0)`, `ctx.s ~= 'x'`, `ctx.n = 1`, and a missing operand.
  - A literal in place of the context path fails flow loading. The
    conditions `true`, `1` and `true == true` were looked up as context keys
    named `true` and `1`, routed false, and skipped the step; use
    `flow:step` for a step that always runs. A key with such a name is
    still reachable as `ctx.true`.
  - String operands are stricter. An unquoted operand must be one word, so
    `ctx.s == hello world` must be written `ctx.s == 'hello world'`. A
    quoted operand cannot contain its own quote character: write
    `"it's"`, not `'it's'`. An empty string is written `''`.
  See [Condition Syntax](../nodes/if_node.md#condition-syntax).

## Security

- **`mcp_client` bounds Streamable HTTP responses** (IF-173). `rmcp` is
  updated to 3.9.0 for RUSTSEC-2026-0462: a JSON response is limited to
  16 MiB and a non-2xx body to 64 KiB, and a larger one fails the action
  instead of growing process memory until the timeout. A server that returned
  larger responses now fails.
- **`extract_xlsx` bounds the workbook style table** (IF-173). `calamine` is
  updated to 0.38.0 for RUSTSEC-2026-0436. That version expands `styles.xml`
  into one record per cell format when a workbook opens, so the table is now
  charged against `IRONFLOW_MAX_XLSX_OUTPUT_BYTES` first. Workbooks within
  Excel's own format limits fit the default. Cell text containing `_xHHHH_`
  escapes is now decoded for every UTF-16 code unit, as Excel does, rather
  than only for `_x00HH_`.
- **One reviewed audit exception** (IF-173). RUSTSEC-2026-0459 in the bundled
  SQLite cannot be fixed until `sqlx` releases support for
  `libsqlite3-sys` 0.39. Do not open SQLite files from untrusted parties with
  `db_query` or `db_exec`; see `docs/SECURITY.md`.
- The dependency audit and license check also run daily in CI, so advisories
  against released code surface without a push.
- **Image nodes, `image_to_pdf` and `transcribe` no longer write through
  symlinks** (IF-179). `image_resize`, `image_crop`, `image_rotate`,
  `image_flip`, `image_grayscale`, `image_watermark`, `image_convert`,
  `image_to_pdf` and the `transcribe` `output_file` now publish through the
  staged atomic writer that `write_file` uses. A destination that is a
  symlink, FIFO or other special file is refused instead of followed, and a
  failed encode, an expired deadline or a cancelled step leaves the previous
  file in place instead of a truncated one. Output bytes are unchanged and the
  destination directory must still exist, and the published mode is unchanged:
  the umask default for a new file and the existing bits for a replaced one
  (IF-185).

## Unreleased changes

- Schedules that share a minute no longer lose occurrences to
  `IRONFLOW_MAX_CONCURRENT_FLOW_LOADS` (IF-182). A scheduled start now waits
  up to 5 seconds for a flow-load permit instead of failing at once, and an
  occurrence that still finds the limit full is retried on later ticks inside
  its grace window rather than dropped. Such an occurrence is logged as
  deferred and counted under the new scheduler outcome label `deferred` in
  `ironflow_scheduler_decisions_total`. API and webhook requests still receive
  `503` immediately at the limit.

- A run created again under the same ID after `DELETE /runs/{id}` now has
  its events and SSE stream (IF-184). Idempotent and scheduled run IDs are
  deterministic, and the event store's deletion fence used to reject every
  later event for the ID: the new run finished normally with an empty
  stream. The fence now belongs to the deleted run, on the memory, SQL and
  Redis event backends, and a `DELETE` that returns `404` no longer fences
  the ID. SQL event stores gain a nullable `incarnation` column on the
  event-deletion table, added at startup; an older replica that is still
  running during a rolling upgrade keeps rejecting events for deleted IDs
  until it is replaced. Custom `EventStore` implementations compile
  unchanged: the new `publish_as`, `begin_incarnation` and
  `delete_orphaned_run` methods have defaults.

- Every release archive and the container image now ship
  `THIRD_PARTY_LICENSES.md`, the generated license notices for the Rust crates
  statically linked into IronFlow, next to the bundled fonts' OFL license
  (IF-204). The file is committed, regenerated with
  `bun --no-env-file scripts/third_party_notices.ts write`, and checked
  against `Cargo.lock` wherever the dependency license gate runs. Its rows
  carry no version, so it changes only when a crate is added or removed or a
  license changes (IF-208).

Earlier changes remain in [the 1.20.0 notes](1.20.0.md).
