# Next IronFlow Release - Draft

The approved 1.21.0 changes are recorded in [1.21.0.md](1.21.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

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

Earlier changes remain in [the 1.20.0 notes](1.20.0.md).
