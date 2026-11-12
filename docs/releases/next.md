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
  destination directory must still exist. The published file now has mode
  `0600` on Unix instead of the umask default, and a replaced file no longer
  keeps its mode (IF-185 tracks that choice); a flow whose outputs are read by
  another user must widen the mode.

## Unreleased changes

- Every release archive and the container image now ship
  `THIRD_PARTY_LICENSES.md`, the generated license notices for the Rust crates
  statically linked into IronFlow, next to the bundled fonts' OFL license
  (IF-204). The file is committed, regenerated with
  `bun --no-env-file scripts/third_party_notices.ts write`, and checked
  against `Cargo.lock` wherever the dependency license gate runs.

Earlier changes remain in [the 1.20.0 notes](1.20.0.md).
