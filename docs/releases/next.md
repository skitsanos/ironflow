# Next IronFlow Release - Draft

The approved 1.22.0 changes are recorded in [1.22.0.md](1.22.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

## Fixed

- `extract_xlsx` returns a cell with an elapsed-time format (`[h]:mm:ss`) as an ISO-8601 duration such as `PT37H30M`. It was returned as a date (`1900-01-01T13:30:00`), which dropped whole days (IF-227). The node page now documents what dates, times of day, durations and 1904-system workbooks return.

Earlier changes remain in [the 1.21.0 notes](1.21.0.md).
