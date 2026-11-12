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

## Unreleased changes

- Every release archive and the container image now ship
  `THIRD_PARTY_LICENSES.md`, the generated license notices for the Rust crates
  statically linked into IronFlow, next to the bundled fonts' OFL license
  (IF-204). The file is committed, regenerated with
  `bun --no-env-file scripts/third_party_notices.ts write`, and checked
  against `Cargo.lock` wherever the dependency license gate runs.

Earlier changes remain in [the 1.20.0 notes](1.20.0.md).
