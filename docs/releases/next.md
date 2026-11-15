# Next IronFlow Release - Draft

The approved 1.21.0 changes are recorded in [1.21.0.md](1.21.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

## Upgrade Notes

- **A configuration variable that is present but empty now stops startup**
  (IF-198). `IRONFLOW_API_KEY=` (empty or whitespace-only) used to take
  priority over `api_key` in `ironflow.yaml` and was then discarded, so a
  server bound to loopback started without authentication while a key was
  configured. It now fails with a message that names the variable. The same
  applies to an empty `api_key` in `ironflow.yaml`, and to
  `IRONFLOW_CORS_ORIGINS`, `IRONFLOW_SQL_TABLE_PREFIX`, `IRONFLOW_STORE`,
  `IRONFLOW_EVENT_STORE`, `REDIS_URL` and `REDIS_PREFIX`: an empty
  `IRONFLOW_SQL_TABLE_PREFIX` previously replaced a configured prefix with the
  default one, an empty `REDIS_PREFIX` selected unprefixed keys, and an empty
  `IRONFLOW_CORS_ORIGINS` replaced the configured origins with none. Before
  deploying, check compose files, `.env` files and orchestrator secrets for
  variables that can resolve to an empty string. To fix a deployment that no
  longer starts, remove the variable (the `ironflow.yaml` field or the
  default then applies) or give it a value. To serve without a key on
  purpose, set `IRONFLOW_ALLOW_UNAUTHENTICATED_API=true`; to deny
  cross-origin requests, leave `IRONFLOW_CORS_ORIGINS` unset. Variables that
  are unset behave as before. See
  [`ironflow serve`](../CLI_REFERENCE.md#ironflow-serve).

- **The default SQLite store accepts any directory name, and in-memory
  SQLite URLs are rejected** (IF-199). The store directory (`--store-dir`,
  `IRONFLOW_STORE_DIR`) used to be formatted into a connection URL, so a
  relative directory with a space failed with `invalid domain character`, a
  `?` in the name failed as an unknown query parameter, and a percent
  sequence was decoded: `runs%41data` opened `runsAdata`, and when that
  directory existed the database was written there, outside the directory
  that had been created and restricted. The directory is now used as a
  filesystem path, for the state and event stores and every command. Stores
  at ordinary paths open as before. If a store directory name contains a
  percent sequence such as `%41` or `%20`, look for `ironflow.sqlite` or
  `ironflow-events.sqlite` in the directory with the decoded name and move
  them into the configured directory while IronFlow is stopped. A store
  directory that is not valid UTF-8 is refused for the SQLite backend.
  `IRONFLOW_STORE_URL` and `IRONFLOW_EVENT_STORE_URL` values such as
  `sqlite::memory:` or `mode=memory` now stop startup with a message that
  names the setting; they never worked, failing later with `duplicate column
  name`, because each pooled connection opened its own empty database. An
  invalid `IRONFLOW_SQL_TABLE_PREFIX` is also reported before the store
  directory or a database file is created. See
  [Store directory and SQLite URLs](../CLI_REFERENCE.md#store-directory-and-sqlite-urls).

- **`artifacts prune` reaches orphans behind referenced artifacts, and
  `--limit` now bounds deletions** (IF-200). The command used to take the
  first `--limit` old artifacts in listing order, skip those still referenced
  and exit. Once that many old artifacts were referenced by retained runs,
  every invocation printed `inspected 100, retained 100, deleted 0` and the
  unreferenced artifacts behind them were never deleted, so storage grew while
  pruning appeared to run. An invocation now examines up to `--scan` stored
  digests (new, default and maximum 10,000) in digest order, skips referenced
  ones without counting them, and deletes up to `--limit` (still 1–100)
  unreferenced ones. It prints a second line: `Artifact prune complete: …`, or
  `Artifact prune incomplete: … continue with --after <sha256>`. Scheduled
  pruning must loop on that digest with the new `--after` flag until the
  command prints `complete`; a single invocation covers the whole store only
  while it holds no more digests than `--scan`. `inspected` in the first line
  now counts candidates resolved, which can exceed `--limit`. A context string
  that starts with `artifact://` without being a canonical URI no longer
  aborts the prune: it is skipped and reported as a count on standard error.
  The first prune after upgrading may delete a backlog that earlier releases
  could not reach. See
  [`ironflow artifacts prune`](../CLI_REFERENCE.md#ironflow-artifacts-prune)
  and [scheduling artifact pruning](../REPLICA_DEPLOYMENT.md#scheduling-artifact-pruning).

- **A blank `IRONFLOW_ARTIFACT_DIR` now stops startup.** The variable was not
  covered by the IF-198 rule above: an empty value let the server start and
  failed only when a step first published an artifact, and a whitespace-only
  value created a directory with that name under the working directory.
  `run`, `validate`, `serve` and `artifacts prune` now fail with a message
  that names the variable. Remove it to use `data/artifacts`, or give it a
  directory. If a deployment ran with a whitespace-only value, its artifacts
  are in that oddly named directory: move its `sha256` folder to the intended
  location before restarting.

- **`llm` rejects an unknown `provider` instead of using OpenAI** (IF-197).
  Every `provider` value other than `openai_compatible`, `compatible`,
  `azure` and `custom` used to select the OpenAI configuration, so a
  misspelling such as `"openai-compatible"` or an unsupported name such as
  `"ollama"` sent the prompt and `OPENAI_API_KEY` to the OpenAI base URL
  instead of the configured gateway. Such a step now fails before any request
  with `llm: unsupported provider '<value>'. Use 'openai',
  'openai_compatible', 'azure', or 'custom'.` Check every `nodes.llm` step
  whose `provider` is not one of those names: it has been calling OpenAI, and
  it now needs `openai_compatible` or `custom` with an explicit `base_url`.
  `ai_embed`, `ai_chunk_semantic` and `transcribe` already rejected unknown
  names and now also reject a `provider` that is not a string.
- **`llm` fails the step when a successful response has no usable reply**
  (IF-197). A 200 response with no text and no tool call (an empty `choices`
  array, empty or null content, a refusal without content, a Responses result
  holding only reasoning items, or a body that is not the expected JSON) used
  to succeed with `{output_key}_text = "<no reply>"`. It now fails with
  `llm: <provider> returned HTTP 200 with no usable reply`, naming the
  `finish_reason` or response `status` when present. A flow that tested for
  the `<no reply>` text should use `on_error` or a step retry instead.
  Tool-call-only replies still succeed with `<tool call requested>`; in
  responses mode `function_call` output items are now reported through the
  `{output_key}_tool_calls*` keys, and text is no longer scraped from the
  identifiers of typed non-message items.
- **`llm` parameters in responses mode** (IF-197). `system_prompt` is now
  sent as `instructions` instead of being dropped. Top-level `tools`,
  `tool_choice` and `response_format` are chat-shaped and were dropped
  silently; in responses mode they now fail the step before a request, and
  the Responses-shaped fields go through `extra` as before. In chat mode the
  documented top-level `response_format` is now sent (it was ignored); a
  `response_format` inside `extra` still works and wins when both are given.
- **Embedding OAuth token handling** (IF-197). A failed token request no
  longer copies the provider's response body, the token URL or parser detail
  into the step error; the message keeps the HTTP status. Token responses are
  capped by `IRONFLOW_MAX_HTTP_BODY_BYTES`. When the embedding endpoint
  answers 401, the cached token is evicted and the request is repeated once
  with a new token, so a revoked token no longer fails every run until it
  expires. The token cache is now keyed by the client secret as well as the
  token URL, client ID and scope: a step that named a cached client ID with a
  different secret used to receive the cached token without authenticating,
  and could send it to its own `base_url`. A token response whose
  `expires_in` is a numeric string, or is absent, is now accepted (it used to
  fail the step); a token without a stated lifetime is not cached.
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
- **Lua pattern matching is bounded and can be interrupted** (IF-213). One
  `string.find`, `string.match`, `string.gmatch` or `string.gsub` call whose
  pattern backtracks without bound, for example `^.-.-.-.-b` on 1,500
  characters, ran for minutes inside the C matcher, where neither
  `:timeout(...)`, `IRONFLOW_LUA_MAX_SECONDS` nor `IRONFLOW_MAX_RUN_SECONDS`
  could end it, and since IF-211 it held its step, its run and their
  admission for that long. These four functions now run on a matcher that
  counts its steps. A Lua state may spend `IRONFLOW_LUA_MAX_PATTERN_STEPS`
  steps (default `50000000`, `0` disables, a malformed value stops startup);
  beyond that the call fails with `Lua pattern matching exceeded step budget
  of 50000000 (IRONFLOW_LUA_MAX_PATTERN_STEPS)`. A step timeout, a
  cancellation and the Lua time budget now end a match in progress, so the
  delay described for IF-211 above no longer applies to pattern matching.
  The pattern syntax and results are those of Lua 5.4. What can change for an
  existing flow: code that matches patterns over more than a few tens of
  megabytes in one step, or uses heavily backtracking patterns that used to
  finish, can reach the budget and needs a higher
  `IRONFLOW_LUA_MAX_PATTERN_STEPS` or a simpler pattern; and an error raised
  by one of these functions in tail position (`return s:find(p)`) no longer
  names the calling line. `string.rep("", n)` returns at once instead of
  counting to `n`; `table.move` over more than 100,000 positions, and
  `table.insert`/`table.remove` on a table with a metatable, run as Lua
  loops under the instruction and time budgets instead of looping in C. See
  [Execution limits](../nodes/code.md#execution-limits).
- **Child steps observe the deadline of the parent step that waits for them**
  (IF-213). With `nodes.subworkflow(...)`:`timeout(30)`, a child step without
  a `timeout` of its own used to see no deadline and was stopped only by the
  cancellation that follows the parent's timeout. The deadline visible to a
  child step's work is now the earlier of its own and the parent step's, at
  every nesting level and for all four composition nodes; a `wait = false`
  child inherits none. Timeout results are unchanged: the parent step still
  reports its own timeout and cancels the child. Work in a child that reads
  the deadline, such as `ai_embed` requests, cooperative blocking workers and
  Lua code, now stops or bounds itself at the parent's deadline. See
  [Parent deadline](../nodes/subworkflow.md#parent-deadline).
- **`markdown_to_html` converts off the async runtime thread** (IF-213). The
  Markdown parser and the optional sanitizer ran on a runtime worker thread,
  where a large input delayed unrelated runs and a step timeout could not
  take effect until the conversion returned. They now run as tracked work on
  the blocking pool like `html_to_markdown`. Output is unchanged.
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

- **`s3vector_query_vectors` rejects a flag that is not a boolean**
  (IF-221). `strict`, `return_metadata` and `return_distance` were read as
  `false` when the value could not be understood, for example `"tru"`, a
  number or a `${ctx.key}` template whose key was missing. A malformed
  `strict` therefore dropped the cosine requirement for `min_similarity`,
  and the query returned unfiltered results from a non-cosine index. Such a
  value now fails the step before any request is sent. Booleans, the strings
  `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`, and templates resolving to
  them are accepted as before. See
  [Boolean Flags](../nodes/s3vector_query_vectors.md#boolean-flags).

- **`s3vector_query_vectors` interpolates the inline `filter`** (IF-221).
  String values inside `filter` were sent as written, so
  `filter = { tenant = "${ctx.tenant_id}" }` asked for the literal text
  `${ctx.tenant_id}` and matched nothing. They are now interpolated once, at
  any depth; object keys are not. A placeholder whose value is missing,
  `null`, an object or an array fails the step before any request is sent,
  instead of becoming an empty string. A filter that must contain a literal
  `${ctx.key}` needs the escape `\${ctx.key}`. A filter read through
  `filter_key` is data and is still used as stored. See
  [Metadata Filter](../nodes/s3vector_query_vectors.md#metadata-filter).

- **`s3vector_*` nodes reject a malformed `endpoint_url` or `region`**
  (IF-221). A value that was not a string, for example a number or a table,
  was ignored, and the request went to `AWS_ENDPOINT_URL`, the region chain
  or the default AWS endpoint instead of the configured one. Such a value,
  and one that is empty after interpolation, now fails the step before the
  client is built. Leave the parameter out to use the environment. `top_k`
  and `dimension` above 2147483647 are refused instead of wrapping to a
  negative number.

- **A child `input` mapping fails when its parent key is absent** (IF-221).
  In `subworkflow`, `parallel_subworkflows` and `repeat_subworkflow` a string
  in `input` names a parent context key. When the key was absent,
  `subworkflow` dropped the entry and ran the child without it, and the other
  two nodes passed the string itself to the child as text. The step now fails
  before any child run starts, with a message that names the mapping entry.
  Two forms state the exceptions: `{ optional = "key" }` omits the entry when
  the key is absent, and `{ literal = value }` passes a value as written.
  **Check every `input` that passes text**, for example
  `input = { name = "Alice" }` in a `parallel_subworkflows` entry: write
  `input = { name = { literal = "Alice" } }`. Numbers, booleans, arrays and
  other objects are still passed as written; only an object with the single
  key `optional` or `literal` is now read as a form. An `input` that is not
  an object is rejected instead of giving the child the whole parent
  context. See [Input mapping](../nodes/subworkflow.md#input-mapping).

- **`tool_dispatch` fails when a handler `input` references a missing
  parent value** (IF-221). A `ctx.<path>` reference that did not resolve was
  passed to the handler as JSON `null`, and a bare string that named no
  parent key was passed as literal text, so a handler could run without the
  user, tenant or account it was meant to act for. Both now fail the step,
  with a message that names the tool and the mapping entry, before the first
  handler of the batch runs; `on_error = "ignore"` does not suppress it.
  Write `{ optional = "ctx.user.id" }` to keep `null` for a value that may be
  absent, and `{ literal = "metric" }` to pass text. References into the
  tool call (`arguments.*`, `call.*`) still resolve to `null` when the model
  did not supply the field. See
  [Tool Handler Mapping](../nodes/tool_dispatch.md#tool-handler-mapping).

- **`xml_stringify` fails on keys that are not XML names, and on array
  sources** (IF-196). Object keys were written as tag and attribute names
  unchecked, so a key from untrusted data could add elements, and a key with
  a space produced a document `xml_parse` rejected. A key, `root_tag` or
  `item_tag` that is not an XML name now fails the step; `prefix:local` names
  and `@xmlns` declarations are accepted. A source value that is an array
  produced no root element or several and now fails unless the new `item_tag`
  parameter names an element for each item; this includes a one-item array,
  which used to yield a document. Number and boolean values of `@attribute`
  and `#text` keys were dropped and are now written as text
  (`{ ["@unit"] = 5, ["#text"] = 3 }` gives `<qty unit="5">3</qty>`, was
  `<qty/>`); an object or array there, and a value with a character XML 1.0
  cannot carry, fail the step. Tabs and line breaks in attribute values are
  written as character references. See
  [`xml_stringify`](../nodes/xml_stringify.md).
- **`xml_parse` rejects a document with more than one root element**
  (IF-196). It used to return the last root and discard the others.
- **`csv_parse` type inference keeps identifiers as text** (IF-196). With
  `infer_types = true`, a field now stays a string when it has a leading zero
  before another digit (`02134`), a leading `+` (`+15551234567`), more than
  15 significant digits (`12345678901234567890`, previously a rounded float;
  16 to 19 digit integers were exact and are strings too), is `-0`, or spells
  a non-finite number (`nan`, `inf`, `infinity`, `1e999`, previously `null`).
  Plain decimal numbers and booleans convert as before. A flow that did
  arithmetic on such a column now receives strings; see the rules under
  [`csv_parse`](../nodes/csv_parse.md#type-inference).
- **`data_filter` `eq` and `neq` compare numbers by value** (IF-196). An
  integer field never equalled a float `value` such as `10 / 2`, so `eq`
  returned nothing and `neq` returned every item while `gte` matched. Both
  now compare numbers numerically, also inside arrays and objects; other
  types stay strict. An `op` the node does not know used to return an empty
  result and now fails the step. `deduplicate` treats `5` and `5.0` as the
  same key for the same reason. See
  [`data_filter`](../nodes/data_filter.md#equality).
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
  - `extract_word` ignores the previous formatting that a tracked formatting
    change keeps under `w:rPrChange` and `w:pPrChange`. Before, a paragraph
    restyled with change tracking on reported its old style, list state and
    run flags instead of the current ones.
  - `extract_pptx` speaker notes contain the body placeholder only. The slide
    number, date, header and footer of the notes page and field values are no
    longer appended, and a slide without notes no longer reports its slide
    number as `speaker_notes`.

  See [`pdf_metadata`](../nodes/pdf_metadata.md),
  [`extract_word`](../nodes/extract_word.md#text-boxes) and
  [`extract_pptx`](../nodes/extract_pptx.md#text-fidelity).

- **Deleting a scheduled run no longer lets its occurrence run again**
  (IF-201). Before, a server that started or restarted inside a schedule's
  `grace_seconds` ran an occurrence a second time when its run record was
  gone. With the Redis state store this happened without any action whenever
  `REDIS_TTL` was shorter than the grace window (`grace_seconds: 86400` with
  `REDIS_TTL=3600`: the 02:00 run expired at about 03:00 and any restart
  before 02:00 the next day ran the flow again). With every state store it
  happened after `DELETE /runs/{id}`. The claim for an instant now records
  that its run was created, and an instant with that record is not started
  again. Deleting a scheduled run therefore removes the record and does not
  release the instant, on SQLite, PostgreSQL, JSON and Redis alike; start the
  flow with `POST /flows/run` or `ironflow run` to run it again. An instant
  claimed by a server that stopped before creating the run is still
  recovered within grace. Three things to know when upgrading:
  - `ironflow serve` now logs a warning for each schedule whose
    `grace_seconds` plus 60 seconds is not below `REDIS_TTL` (Redis state
    store only). Startup continues; raise `REDIS_TTL` or lower
    `grace_seconds` to clear it.
  - SQL state stores gain a nullable `run_micros` column on the
    `schedule_claims` table at first start. Older binaries keep working
    against the upgraded table, but only upgraded replicas write and read
    the record, so the guarantee holds once every replica is upgraded.
    Occurrences that ran before the upgrade have no record.
  - The storage metrics gain the state-store operations
    `record_schedule_run` and `schedule_run_recorded`.

  A custom `StateStore` keeps compiling: the two new trait methods have
  defaults that record nothing, which leaves such a store fenced by the run
  record alone, as before. See
  [Record retention and deleted runs](../CLI_REFERENCE.md#record-retention-and-deleted-runs).

- **Parsing, planning and fan-out paths have explicit work budgets**
  (IF-215). Each of these was unbounded, or bounded only by a library
  constant, and now fails the step with an error that names the limit. The
  defaults are listed under
  [Resource limits](../CLI_REFERENCE.md#environment-variables); a malformed
  value stops startup.
  - `extract_html` with `format = "text"` accepts at most
    `IRONFLOW_MAX_HTML_TEXT_DEPTH` levels of element nesting (default
    `1024`). Deeper markup was accepted before and parsed in time that grows
    with the square of the depth. Raise the limit for documents that
    legitimately nest deeper.
  - Flow validation plans phases in time linear in the number of steps and
    dependencies. A 10,000-step dependency chain took about 10 s to
    validate in an unoptimized build and now takes milliseconds; phases and
    cycle errors are unchanged, so this needs no limit.
  - `html_to_markdown` and `extract_html` with `format = "markdown"` charge
    their Markdown against `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` (default
    50 MiB) while converting. Nested block quotes and lists repeat a prefix
    on every line and a table with a header row pads every cell to its
    column's widest cell: 600 KB of nested quotes produced 73 MB and a
    1 MiB header cell above 200 rows produced 212 MB. `html_to_markdown`
    output was not limited at all before. Such a document now fails with
    `Markdown output exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES`.
  - `mcp_client` refuses a result larger than
    `IRONFLOW_MAX_MCP_RESULT_BYTES` (default 4 MiB) before it enters
    workflow context. Until now only the transport limits applied, so a
    16 MiB reply was written to context under up to four keys. A flow that
    relies on larger `initialize`, `list_tools` or `call_tool` results must
    raise the limit.
  - The `json_parse` node and `json_extract_path` with `parse_json` count
    the values in their text against `IRONFLOW_MAX_JSON_PARSE_NODES`
    (default 1,000,000) before building anything. A document used to be
    built whole whatever its size, at many times the size of its text. The
    budget is separate from the Lua conversion budget, so a native value
    may still be larger than a handler can convert. A flow that parses
    larger documents with these nodes must raise the limit.
  - One `s3_list_objects` page and one `s3_list_buckets` response may be at
    most `IRONFLOW_MAX_S3_LIST_PAGE_BYTES` (default 16 MiB). The SDK reads a
    list response whole, so an S3-compatible endpoint could send one very
    large page; it is now refused before it is buffered. Real S3 pages are
    far below the default. The S3 listing of `ironflow artifacts prune` has
    the same limit.
  - The retry ceiling of 100 per step, which validation already enforced,
    is now documented and covered by a regression. With one error record
    of at most 64 KiB per failed attempt, a step adds at most 101 of them.
  - At most `IRONFLOW_MAX_LIVE_CHILD_RUNS` child runs (default 4096) are
    alive at once in the process, across all composition nodes, nesting
    levels and top-level runs. Wide `parallel_subworkflows` at several
    levels multiplied without a bound before. A child that finds no free
    slot is refused with `child workflow run limit reached` and fails its
    step; lower `max_concurrent` or raise the limit for a deployment that
    really runs more children at once.
  - `ai_chunk_semantic` refuses `sim_window` above 101, `sg_window` above
    1001 and `poly_order` above 16, before any embedding request. Larger
    values were accepted and cost CPU time on the async runtime that grows
    with sentences × window and with the cube of the order (330 s for order
    3,200 in an optimized build). An order at or above the window still
    disables smoothing, as before; only values above the ceilings fail.

- **`pdf_to_image` and `pdf_thumbnail` no longer need Pdfium** (IF-194). Both
  nodes now draw pages with a built-in pure-Rust renderer, so they work in
  every default build and in the container image with no native library.
  Previously they needed a Pdfium shared library and, with the `pdfium-render`
  0.9 binding, could render only once per process: every later render failed
  with `PdfiumLibraryBindingsAlreadyInitialized`. Parameters, outputs and the
  DPI, page and pixel limits are unchanged. What changes for existing
  deployments:
  - Pixels differ from Pdfium output (anti-aliasing, hinting, image
    resampling), so do not compare artifacts byte for byte across the upgrade.
    The known rendering gaps are listed under
    [Renderer](../nodes/pdf_to_image.md#renderer).
  - The shared [PDF loading limits](../PDF_LOADING.md) now apply to both
    nodes: a password-protected file fails with `PDF requires a password`, and
    a file that strict parsing rejects fails with `failed to parse PDF`
    instead of being repaired by Pdfium.
  - A page is admitted before it is drawn. The new
    `IRONFLOW_MAX_PDF_RENDER_OPERATIONS` (default `1000000`) bounds its drawing
    operations, `IRONFLOW_MAX_IMAGE_PIXELS` bounds each embedded image, and
    the streams a page uses are bounded by
    `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES`. A dense vector page (a large
    map or drawing) that fails with the operations message needs the limit
    raised.
  - Pdfium support is removed, not kept as an option: the `pdfium-render`
    dependency is gone and `PDFIUM_LIB_PATH` is ignored. A Pdfium library
    installed only for IronFlow can be uninstalled.

## Security

- **The outbound destination guard can be forced on for the whole process,
  and covers every node that connects to a flow-chosen destination**
  (IF-212). `IRONFLOW_BLOCK_PRIVATE_NETWORK=true` (or
  `block_private_network: true` in `ironflow.yaml`) refuses `localhost` and
  every address that is not a public unicast destination, before any
  connection, for the `http_*` nodes, `llm` (including an absolute
  `chat_path`, `responses_path` or `path` of a custom provider), `ai_embed`
  and `ai_chunk_semantic` (including the OAuth `token_url`), `transcribe`,
  `slack_notification`, `send_email` (Resend and SMTP), `arangodb_aql`,
  `mcp_client` over Streamable HTTP and the S3 and S3 Vectors nodes, with
  every DNS answer validated and pinned. `db_query` and `db_exec` get a host
  check only, because the SQL driver resolves the name again. The policy is
  one-way: `block_private_network = false` on an `http_*` node cannot turn
  it off. It is off by default, and with it off nothing changes.
  `IRONFLOW_PRIVATE_NETWORK_ALLOWLIST` (or `private_network_allowlist`)
  names the internal destinations that stay reachable, as host names,
  `host:port`, addresses and CIDR ranges; only the operator can set it. With
  the policy on, a local model server (`OLLAMA_HOST`, a loopback
  `base_url`), MinIO, ArangoDB, a mail relay or a database on a private
  address needs an entry, for example `127.0.0.1:11434`, and the covered
  nodes connect directly, ignoring a system proxy. A malformed value of
  either setting stops `run`, `validate` and `serve` at startup. The
  operator endpoints (artifact store, state and event stores, metrics) are
  not checked. See
  [Outbound destinations](../SECURITY.md#outbound-destinations).
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

- A step timeout or retry backoff that cannot be scheduled now fails
  validation (IF-203). `:timeout(1e19)` and `:retries(1, 2e19)` used to pass
  `ironflow validate` and then end the run `stalled` with `execution ended
  with non-terminal task state`. Validation now reports `Step '<name>' timeout
  is too large to schedule` or `Step '<name>' retry backoff is too large to
  schedule: the wait before retry <n> is not representable as a duration`, and
  no run starts. The wait before retry *n* is `backoff × 2^(n-1)` seconds and
  every wait up to the retry count is checked, so `:retries(100, 1)` is
  refused (retry 65 would wait 2^64 seconds) while `:retries(64, 1)` and any
  count with a zero backoff are accepted. A flow that is refused by this check
  could not have completed those retries before. See
  [Retries](../LUA_FLOW_GUIDE.md#retries).

- `POST /flows/run` answers an invalid flow with `400` and the validation
  errors (IF-203). A flow whose step depends on an undefined step, names an
  unknown node type or has a step option out of range used to reach the
  engine, which answered a broken graph with `500 internal_error`. The
  response is now `400 bad_request` with `Flow validation failed: ` and the
  errors `POST /flows/validate` reports, and no run record is created. A
  webhook whose configured flow is invalid answers `400` with `Webhook flow
  failed validation` and logs the errors. See
  [Ad-hoc Flow Execution](../CLI_REFERENCE.md#ad-hoc-flow-execution).

- API requests and webhook bodies can no longer make a schedule skip its
  instants (IF-203). The overlap check recognised a schedule's running run by
  the `_schedule` context key alone, which a caller could set. It now
  requires the run ID the scheduler derives for the occurrence, which API and
  webhook runs never have, and `POST /flows/run` and webhooks answer `400`
  for a context that defines `_schedule` or `_schedule_instant`. Remove those
  keys from request contexts and webhook payloads; no other key is affected.

- A run that finishes normally is no longer reported as a lost lease
  (IF-203). A lease renewal between the terminal status write and the end of
  finalization logged `workflow run ownership lease expired or was lost`,
  incremented `ironflow_lease_events_total{outcome="lost"}` and started a
  reconciliation pass, on every store. Run state was not affected. Alerts on
  that series no longer see these false positives.

- Starting a PostgreSQL replica no longer locks tables that running replicas
  use (IF-202). Every start used to take `ACCESS EXCLUSIVE` on the events
  table before checking its primary key, and to issue
  `CREATE INDEX IF NOT EXISTS` for indexes that already existed, which
  requests `SHARE` on the table. A starting replica waited for any session
  using those tables (a dump, a long query, an idle transaction, for the
  indexes any open writer), and the replicas in service queued behind its
  pending lock: events were dropped after 500 ms and SSE polls stalled. An
  up-to-date schema is now verified through the catalog only. Changes that
  need a table lock (the legacy event identity migration, a missing index or
  column) wait at most 5 seconds and then stop startup with a message that
  names the table and the change; nothing is changed and the next start
  retries. See
  [PostgreSQL startup and schema locks](../REPLICA_DEPLOYMENT.md#postgresql-startup-and-schema-locks).
  The startup repair of event sequence counters also no longer rewrites
  every counter row: on a test table with 50,000 runs it wrote 50,000 rows
  and 15 MB of WAL per start, and now writes none when the counters are
  current. No configuration or migration is required.

- `DELETE /runs/{id}` on PostgreSQL no longer removes a run that was created
  again under the same ID while the delete was in progress (IF-202). The
  delete read the lease before it locked the run row, so a new run and its
  owner's lease committed between the two statements were deleted while the
  owner was executing. Such a delete now returns `409`, as for any run that
  is still executing. SQLite was not affected.

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

- A `DELETE /runs/{id}` that overlaps a new run with the same ID no longer
  removes that run's events (IF-214). State and events are deleted in two
  steps; a scheduled occurrence or a retried idempotent request accepted
  between them used to lose its first events and be fenced, and the
  documents told callers to wait for the `DELETE` response. Every event
  backend now records which run owns a stream when the run starts, the SQL
  and Redis state stores report which run a delete removed, and the event
  delete leaves a stream owned by a newer run alone. Once a run has started,
  only that run may publish to its stream: a publisher that names another
  run, or none, is rejected even if the ID was never deleted. SQL state
  stores gain a nullable `incarnation` column on the runs table, added at
  startup; SQL event stores write one row per run to the event-deletion
  table, and Redis keeps one `event_incarnations:v1:` key per run for the
  stream's retention. Custom stores compile unchanged: the new
  `StateStore::delete_run_reporting` and `EventStore::admitted_incarnation`,
  `delete_incarnation` and `delete_orphaned_incarnation` have defaults that
  keep the previous behaviour.

- A run whose event stream cannot be opened no longer runs silently
  (IF-214). Opening the stream was attempted once with a 2-second bound and
  a failure was only logged: a run created again under a deleted ID then
  executed with no events and a silent SSE stream. It is now attempted up to
  three times, one second each. The new counter
  `ironflow_event_admissions_total` has the outcomes `admitted`,
  `admitted_after_retry` and `failed`, and a run that was not admitted
  carries the context key `_event_stream_warning`, visible through
  `GET /runs/{id}`. See [metrics](../METRICS.md).

- A Redis legacy event stream that expires while it is being migrated is no
  longer reported as corrupt (IF-214). The call that found the quarantined
  stream gone used to fail once with `Blocked Redis legacy event migration`;
  the next call returned an empty stream. The call now returns the empty
  stream itself, or `409 conflict` with retry guidance while the recorded
  expiry is still ahead. No data was lost before and none is now.

- Every release archive and the container image now ship
  `THIRD_PARTY_LICENSES.md`, the generated license notices for the Rust crates
  statically linked into IronFlow, next to the bundled fonts' OFL license
  (IF-204). The file is committed, regenerated with
  `bun --no-env-file scripts/third_party_notices.ts write`, and checked
  against `Cargo.lock` wherever the dependency license gate runs. Its rows
  carry no version, so it changes only when a crate is added or removed or a
  license changes (IF-208).

Earlier changes remain in [the 1.20.0 notes](1.20.0.md).
