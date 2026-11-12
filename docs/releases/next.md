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

- Every release archive and the container image now ship
  `THIRD_PARTY_LICENSES.md`, the generated license notices for the Rust crates
  statically linked into IronFlow, next to the bundled fonts' OFL license
  (IF-204). The file is committed, regenerated with
  `bun --no-env-file scripts/third_party_notices.ts write`, and checked
  against `Cargo.lock` wherever the dependency license gate runs. Its rows
  carry no version, so it changes only when a crate is added or removed or a
  license changes (IF-208).

Earlier changes remain in [the 1.20.0 notes](1.20.0.md).
