# Next IronFlow Release - Draft

The approved 1.23.0 changes are recorded in [1.23.0.md](1.23.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

## Upgrade Notes

- **UTF-8 response headers are strings** (IF-264). In `{output_key}_headers`
  of `http_request` and its wrappers, a value whose bytes are valid UTF-8,
  such as `filename="résumé.pdf"` in `Content-Disposition`, is now a string.
  Since 1.23.0 it was `{"base64": "..."}` because it was not plain ASCII. A
  flow that decoded that object for such a header must read the string.
  Values that are not valid UTF-8, or that hold a C1 control character,
  keep the `{"base64": "..."}` form.
- **SQLite stores use WAL** (IF-267). The SQLite state and event stores now
  switch every database file they open, new and existing, to write-ahead
  logging, so a task update no longer waits for open run inspections and
  listings or fails after the 5 s busy timeout behind one. Commits remain
  `synchronous = FULL`. The store directory now also holds
  `ironflow.sqlite-wal` and `ironflow.sqlite-shm`: keep them with the
  database, and back up with `sqlite3 ironflow.sqlite ".backup ..."`,
  `VACUUM INTO`, or a copy of the whole directory while IronFlow is stopped,
  since the database file alone can miss recent commits. For a database on a
  network filesystem set `IRONFLOW_SQLITE_WAL=false`, which switches the file
  back to the rollback journal or refuses to open if SQLite cannot switch.
  Any value other than `true` or `false` is rejected before the default store
  directory or database is created. On Unix a new default `ironflow.sqlite` or
  `ironflow-events.sqlite` is created with mode `0600`, so its side files
  are owner-only too; an existing file keeps its mode inside the `0700`
  store directory (`chmod 600` it to match). CLI commands and a gracefully
  stopped `serve` close their store connections before exiting, so the side
  files are removed at a clean exit. Embedders can do the same with the new
  `StateStore::close` and `EventStore::close`, which default to a no-op.

Earlier changes remain in [the 1.22.0 notes](1.22.0.md).
