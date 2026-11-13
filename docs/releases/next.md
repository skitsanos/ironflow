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
- **A timed-out or cancelled step waits for its Lua code to return**
  (IF-211). A function step, a `nodes.code` step or a `foreach` transform
  that was inside one long C library call, for example a `string.find` whose
  pattern backtracks heavily, did not hold its step: at the timeout the
  `on_error` handler started, dependent steps ran and the run could finish
  while the Lua call was still running, and `ironflow run` then waited out
  the five-second exit bound. The same applied to such a function in a child
  flow and to the top-level code of a child flow while it loads. The step now
  ends only when the Lua call has returned, as stuck built-in blocking nodes
  already did, so a handler no longer races the work it replaces and task and
  run admission are not released early. The visible change is timing: a
  step stuck in such a call reports its timeout, and its run ends, later than
  the configured timeout by the time the call still needs. Ordinary Lua
  loops are unaffected; they are stopped at the next instruction-hook check
  as before, and steps that finish in time behave as before. See
  [Execution limits](../nodes/code.md#execution-limits).
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

- **Some unusual ZIP layouts are now refused** (IF-190). `zip_list`,
  `zip_extract`, `extract_word`, `extract_pptx` and `extract_xlsx` accept
  only the last end-of-central-directory record in the file, and the central
  directory must end exactly where that record (or its ZIP64 record) begins.
  Three shapes that could be accepted before now fail before any entry is
  read: an archive comment that contains a second complete end record (the
  signature `PK\x05\x06` followed by at least 18 more bytes whose own
  comment length fits in the file), a gap between the central directory and
  its end record, and an end record followed by further bytes that an earlier
  record's comment covered. Ordinary archive comments, comments that merely
  contain the signature bytes near their end, and ZIP64 archives open as
  before. Archives with prepended data (self-extracting stubs) were already
  rejected and still are. Rewrite an affected archive with any standard ZIP
  tool to make it readable.

- **`block_private_network` refuses more address ranges** (IF-191). The
  opt-in guard on the `http_*` nodes refused RFC 1918, loopback, link-local,
  unique-local and IPv4-mapped addresses only. It now refuses every address
  that is not a public unicast destination: additionally `100.64.0.0/10`
  (carrier-grade NAT, overlay networks and one provider's metadata endpoint),
  the rest of `0.0.0.0/8`, `192.0.0.0/24`, `198.18.0.0/15`, the documentation
  ranges, `192.88.99.0/24`, multicast and `240.0.0.0/4`; internal addresses
  wrapped in NAT64 (`64:ff9b::/96`), 6to4 (`2002::/16`) or the
  IPv4-compatible form; local-use NAT64, Teredo, IPv6 documentation,
  site-local, multicast and all IPv6 space outside `2000::/3`. The check
  applies to URL literals, redirect targets and DNS answers alike. A flow
  that sets the option and calls a service in one of these ranges, for
  example over a `100.64.0.0/10` overlay network, now fails with `blocked:
  target is a private network address`; remove the option for that step.
  Flows that do not set the option, and public destinations including those
  reached through NAT64, are unaffected. The full list is under
  [Private-network blocking](../nodes/http_request.md#private-network-blocking).
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

- **`s3_list_objects` returns at most 10,000 entries per call** (IF-189).
  `max_keys` was only a page size: the node followed every page and returned
  the whole listing, whatever its size. One call now returns at most
  `IRONFLOW_MAX_S3_LIST_OBJECTS` objects and common prefixes (default
  `10000`). A step that lists more fails with `listing holds more than N
  entries` rather than returning a part. Raise the variable for a trusted
  bucket, or page with the new `max_objects` and `continuation_token`
  parameters and the new `{output_key}_truncated` and
  `{output_key}_next_continuation_token` outputs. Listings within the
  ceiling return the same objects as before, plus
  `{output_key}_common_prefixes`. See
  [`s3_list_objects`](../nodes/s3_list_objects.md#limits).
- **`date_format` applies the offset in an input parsed with `input_format`**
  (IF-195). With `input_format`, the node read the date and time and
  discarded a parsed offset: `2024-06-15 10:30:00 +0200` with
  `%Y-%m-%d %H:%M:%S %z` gave the instant `10:30:00Z` (unix `1718447400`). It
  now gives `08:30:00Z` (`1718440200`), the same as the RFC 3339 form of that
  value, for `%z`, `%:z`, `%#z` and `%+`. Without `timezone` the result keeps
  the input's offset, so the formatted wall-clock time is unchanged and only
  `{output_key}_unix` and `timezone` conversions move; a date-only format
  with an offset is midnight at that offset. A zone name read with `%Z` was
  ignored as well: `UTC`, `GMT`, `UT` and `Z` are still read as UTC, and any
  other name (`CEST`) now fails the step unless the format also has a numeric
  offset. Inputs without an offset are unchanged. Review flows that pass an
  offset through `input_format` and compensate downstream. See
  [`date_format`](../nodes/date_format.md#input-formats-and-offsets).
- **Invalid `date_format` and `html_sanitize` parameters fail the step
  instead of stalling the run** (IF-195). An `output_format` with an unknown
  or incomplete specifier (`%i`, a trailing `%`, `%03d`, `%#z`), a `timezone`
  whose hours overflow, and `allowed_tags` containing `script` or `style`
  made the node panic: the run ended `stalled` and neither retries nor
  `on_error` ran. Each is now a node error. Two `allowed_tags` values that
  used to fall back silently to the default allow-list change result:
  `allowed_tags = {}` now keeps no tags, and a value that is not a list of
  strings (a string, a keyed table, a list with a non-string entry) fails
  the step. See [`html_sanitize`](../nodes/html_sanitize.md#allowed-tags).

- **`s3vector_query_vectors` fails when `filter_key` does not resolve to a
  filter** (IF-192). A `filter_key` naming a context key that was absent, for
  example because the step that builds a tenant filter was skipped or the key
  was misspelled, was ignored: the query ran without a filter and returned
  the nearest vectors from the whole index. The step now fails before any
  request is sent when the key is missing, when its value is `null`, an empty
  object or not an object, and when `filter_key` itself is not a non-empty
  string. A flow that relied on the fallback to run unfiltered must leave
  `filter_key` out of that step, for example by routing to a second query
  step without it. A key holding a non-empty filter object and the inline
  `filter` parameter behave as before. See
  [Metadata Filter](../nodes/s3vector_query_vectors.md#metadata-filter).

- **Extracted text changes for some PDF, DOCX and PPTX documents** (IF-193).
  Review stored extractions, embeddings and assertions that depend on the
  earlier output:
  - `pdf_metadata` and `extract_pdf` metadata decode Info strings as PDF text
    strings. A UTF-16 `title` such as `Müller Report` was returned as
    replacement characters followed by NUL-separated letters and is now the
    readable text; PDFDocEncoding accents and UTF-8 strings with a byte-order
    mark are decoded as well. NUL and other control characters no longer
    reach the run context, and `pdf_metadata` now reads indirect Info values.
  - `extract_word` keeps the text of a paragraph that anchors a text box and
    emits the box paragraphs after it, once. Before, only the box text
    survived, and it was repeated when the document carried a fallback copy.
  - `extract_word` no longer reports a run as bold, italic or struck through
    when the property is switched off (`<w:b w:val="0"/>`), nor as underlined
    for `<w:u w:val="none"/>`. Markdown loses the wrong `**`/`*` markers and
    JSON runs lose the wrong flags. The repository's sample document changes
    here: its table body cells are no longer bold.
  - `extract_pptx` speaker notes contain the body placeholder only. The slide
    number, date, header and footer of the notes page and field values are no
    longer appended, and a slide without notes no longer reports its slide
    number as `speaker_notes`.

  See [`pdf_metadata`](../nodes/pdf_metadata.md),
  [`extract_word`](../nodes/extract_word.md#text-boxes) and
  [`extract_pptx`](../nodes/extract_pptx.md#text-fidelity).

## Security

- **`mcp_client` bounds Streamable HTTP responses** (IF-173). `rmcp` is
  updated to 3.9.0 for RUSTSEC-2026-0462: a JSON response is limited to
  16 MiB and a non-2xx body to 64 KiB, and a larger one fails the action
  instead of growing process memory until the timeout. A server that returned
  larger responses now fails.
- **ZIP admission and the ZIP reader now read the same directory**
  (IF-190). A crafted archive with two end records made the admission scan
  validate a one-entry directory while the reader materialized another, so
  the raw entry, duplicate-name and metadata budgets could be bypassed in
  `zip_list`, `zip_extract` and OOXML/XLSX extraction. Such archives are
  rejected, and the opened archive is checked against the validated
  directory before use.
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
- **`s3_list_objects` cannot be kept paging by an endpoint** (IF-189). A
  truncated page without a continuation token used to restart the listing
  from the first page without end. That response, a repeated token, a page
  larger than requested and 16 consecutive truncated pages without an entry
  now fail the step. These shapes come from S3-compatible services reached
  through `endpoint_url`.
- **The Lua `json_parse()` global is bounded while it parses** (IF-189). The
  text was parsed into a complete tree outside the Lua memory limit before
  the conversion budget applied: a 40 MB array of twenty million numbers
  raised the process to about 750 MB under the default 128 MiB Lua limit.
  Values are now built inside the Lua state as the text is read, and
  construction stops at `IRONFLOW_MAX_CONVERSION_NODES`,
  `IRONFLOW_MAX_CONVERSION_DEPTH` or the Lua memory limit; the same input
  now peaks near 120 MB, most of it the text itself. Results for text within
  the budgets are unchanged. Two details differ for text over a budget: the
  path in the error is the first value over it in document order rather than
  in sorted key order, and values under a repeated object key all count
  toward the node budget.
- **Task error text stays bounded in the store** (IF-189). Since IF-177 a
  task error keeps its first 64 KiB and ends with `...[truncated]` before it
  is stored, published in `TaskFailed` or passed to a recovery step as
  `_error_message`. A regression now holds a run whose step raises an 8 MiB
  error three times to a store under 1 MiB.

## Unreleased changes

- An artifact URI written in an `http_*` `body` (with
  `body_type = "artifact"`) or in a multipart part's `artifact` now supports
  `${ctx.key}` interpolation, resolved once, matching the `write_file`
  `artifact` parameter (IF-186). Such a string was rejected as an invalid URI
  before, so `body = "${ctx.report_artifact.artifact_uri}"` now uploads the
  artifact. Values read through `body_key` and a part's `source_key` are
  still used as stored, and a path is still rejected.

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
