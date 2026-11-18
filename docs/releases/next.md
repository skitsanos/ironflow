# Next IronFlow Release - Draft

The approved 1.22.0 changes are recorded in [1.22.0.md](1.22.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

## Changed

- Schedule time zones are resolved by `jiff` instead of `chrono-tz`, which is
  reported as unmaintained (IF-231). Fire instants and claim keys are
  unchanged: both implementations were compared over 31 zones for 2012 to
  2035 and over the daylight-saving, fractional-offset and skipped-day cases.
  Rolling upgrades need no store migration.
- The bundled IANA time zone data moves from 2025b to 2026f. Among the
  compared zones this changes `America/Havana` only: the 2026f data keeps
  Cuba on daylight time through the winter of 2026 to 2027, so between
  1 November 2026 and 14 March 2027 a schedule in that zone fires one hour
  earlier in UTC than under 1.22.0, at the same wall-clock time. Zone data
  never comes from the host; see
  [Time zones](../CLI_REFERENCE.md#time-zones).
- Library API: the scheduler's time types are `jiff` types instead of
  `chrono` ones (IF-231). `ScheduleConfig::timezone()` returns `&ScheduleZone`
  instead of `chrono_tz::Tz`. `Scheduler::new` and `Scheduler::evaluate` take
  a `jiff::Timestamp`. `CronSchedule::after`, `due_instants`, `resolve_local`
  and `claim_key` take wall-clock times as `jiff::civil::DateTime`, the last
  two with a `&ScheduleZone`, and `DueInstant::instant` is a
  `jiff::ZonedDateTime`. `resolve_local` returns `None` for a wall-clock time
  in the last 26 hours of the year 9999, which no schedule reaches. The CLI,
  `ironflow.yaml`, fire instants, claim keys and stored records are
  unaffected.

- Stored and published timestamps are produced by `jiff` instead of `chrono`
  (IF-231), with no change to any of them: run, task and event records in the
  JSON, SQLite, PostgreSQL and Redis stores, API and SSE bodies, run-list
  cursors, `ironflow list` output and Lua `now_rfc3339()` keep their exact
  text, and run ordering keeps its microsecond keys. Records written by
  earlier versions load unchanged and replicas on both versions can share a
  store during a rolling upgrade. Golden tests recorded from the previous
  implementation cover each form on all four stores.
- Timestamps outside the years -9999 to 9999 are no longer accepted. IronFlow
  never wrote one, since it stores only clock readings; a record edited by
  hand to carry one is now reported as corrupt, and
  `ironflow artifacts prune --before` refuses one. Hand-edited timestamps
  with surrounding spaces, a one-digit hour or more than nine fraction digits
  are refused as well. SQL run and event rows dated before the year 0000,
  which earlier versions wrote but could not read back, now load.
- `ironflow artifacts prune --before` and the embeddings `Retry-After` date
  are parsed by `jiff`. RFC 3339 instants with up to nine fraction digits
  and HTTP dates are accepted as before. `--before` refuses a tenth fraction
  digit, which used to be dropped, and accepts a few more ISO 8601
  spellings, such as a time without seconds.
- Library API: `StateStore::prune_before` compares whole microseconds in
  every store (IF-232). The JSON and Redis stores compared nanoseconds, so a
  cutoff later in a run's own microsecond deleted a run that SQLite and
  PostgreSQL kept; all four keep it now. IronFlow itself does not call this
  method.
- Library API: `RunInfo`, `RunSummary` and `TaskState` (`started`,
  `finished`), `RunEvent::timestamp`, `RunCursor::started`, `RunLease::at`
  and `expires_at`, `StateStore::prune_before` and
  `reconcile_expired_run_leases` use `jiff::Timestamp` instead of
  `chrono::DateTime<Utc>` (IF-231). `ironflow::util::timestamp` holds the
  text and integer forms; `jiff`'s own `Display` and `as_microsecond` differ
  from them and must not be used for stored values.

## Dependencies

- `chrono-tz` is removed and its audit exception (RUSTSEC-2026-0555) retired;
  `jiff` 1.0 is added with only its bundled time zone database enabled.
- `chrono` no longer has its `serde` feature enabled. IronFlow uses it only
  for the `date_format` node and to exchange values with `cron`; it stays in
  the dependency graph, and its audit exception (RUSTSEC-2026-0543) stays,
  through those and `calamine`, `lopdf` and `rmcp`.
- The Markdown parser `comrak` moves from 0.59 to 1.0. It caps the text that
  reference links may expand to at the larger of the input size and 100 KB;
  0.59 used the smaller of the two. A Markdown document that repeats a
  reference link with a long target can therefore produce a longer
  `markdown_to_pdf` or `markdown_to_docx` document than before: 600 links to
  one 1.9 KB target in a 9 KB input give 32 pages, where 0.59 gave fewer than
  10. The existing limits (256 KB input, 20,000 nodes, 32 MiB output) still
  apply.
- `zip` 9.6.1 (re-read recorded in IF-218), `noyalib` 0.0.62, the AWS SDK
  crates, `clap` 4.6.8 and `uuid` 1.29.3.

## Fixed

- `extract_xlsx` returns a cell with an elapsed-time format (`[h]:mm:ss`) as an ISO-8601 duration such as `PT37H30M`. It was returned as a date (`1900-01-01T13:30:00`), which dropped whole days (IF-227). The node page now documents what dates, times of day, durations and 1904-system workbooks return.

## Added

- `extract_word` extracts footnotes and endnotes by default: a reference is
  marked `[^1]` (endnotes `[^e1]`) at its position, labels follow reference
  order, and the referenced notes are listed after the body as
  `[^label]: text` (GFM footnote syntax) or, in JSON, under `footnotes` and
  `endnotes`, with a `note` object on the referencing run. `notes = false`
  turns this off. The new `headers_footers = true` also extracts the header
  and footer parts the sections name: before and after the body in text and
  Markdown, `headers` and `footers` arrays in JSON. It is off by default.
  Parts are resolved through the document relationships (IF-225 (c)). A
  custom footnote mark (`w:customMarkFollows`) is reported as `mark` in JSON
  instead of being repeated after the `[^label]` marker.

  Parts are resolved through the document relationships (IF-225 (c)).
- `extract_pptx` reads PowerPoint modern comments, the format current
  PowerPoint and PowerPoint for the web write. Such decks used to return no
  comments. Each comment carries its author, initials, creation date, text,
  `status` when set (for example `resolved`) and nested `replies`, in JSON,
  the `comments_key` array, and the text and Markdown renderings. When a slide
  has both a modern and a legacy comments part, only the modern one is read
  (IF-226).

## Fixes

- `extract_word` and `extract_pptx` find the core-properties part through the
  package relationship in `_rels/.rels` instead of only at
  `docProps/core.xml`, so `metadata_key` is no longer empty for documents
  saved by Office for the web, which store it as a `.psmdcp` part
  (IF-225 (a)).
- Core-property fields are matched by namespace instead of by prefix, so
  `last_modified_by` is returned when Office for the web writes
  `lastModifiedBy` unprefixed in the default namespace, and an element from
  another namespace that reuses a `dc:` or `cp:` prefix is ignored (IF-225).
- `extract_word` Markdown merges adjacent runs with the same emphasis, keeps
  surrounding whitespace outside the markers and opens no marker for
  whitespace-only runs, so Word's run splits no longer produce emphasis that
  CommonMark leaves as literal asterisks (`*finished**units** *`,
  `**expects **`) (IF-225 (b)).
- `extract_word` finds the main document through the package
  `officeDocument` relationship and its styles, numbering, theme and comments
  parts through the document's relationships, instead of fixed `word/...`
  paths, which remain the fallback. `extract_pptx` likewise finds the
  presentation part through the package relationship instead of only at
  `ppt/presentation.xml`. Both nodes now read `_rels/.rels` on every call,
  so a malformed one fails the step (IF-230).

Earlier changes remain in [the 1.21.0 notes](1.21.0.md).
