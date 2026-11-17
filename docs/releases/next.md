# Next IronFlow Release - Draft

The approved 1.22.0 changes are recorded in [1.22.0.md](1.22.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

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
  Parts are resolved through the document relationships (IF-225 (c)).

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

Earlier changes remain in [the 1.21.0 notes](1.21.0.md).
