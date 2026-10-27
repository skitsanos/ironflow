# Next IronFlow release - draft

Unreleased additions on `develop`, after published **v1.19.2**. The local
package version is `1.19.2-dev.5` for the current integration batch.
This draft does not announce another stable release.

## Document Generation

- Add `markdown_to_pdf` and `markdown_to_docx` for local generation from one
  Markdown source, with tables, code, inline formatting and explicit local or
  artifact image bindings. No external renderer is required for generation.
- Share bounded parsing, process-wide worker admission, cancellation checkpoints
  and atomic output publication. Existing files require `overwrite = true`.
- PDF uses bundled Noto Serif/Mono, Arabic, Hebrew and pan-CJK fonts with
  Parley/Krilla bidi-aware layout. Ukrainian, Chinese, Japanese, Korean, Hebrew,
  Arabic and mixed-direction samples are covered. Unsupported glyphs still fail.
  Logical ActualText spans preserve source Unicode for compatible readers.
- Add `language` and `direction` options, script-specific DOCX fonts/languages,
  bidi paragraphs/runs and RTL tables. DOCX fonts are not embedded, so viewer
  font availability still matters. Pagination remains viewer-dependent.
- Give DOCX body paragraphs an explicit default Normal style, and supply
  physical right-alignment fallbacks for RTL paragraphs/headings in simpler
  importers while retaining logical alignment for modern readers. Code stays
  explicitly LTR and left-aligned, including under `direction = "rtl"`.
- Specify DOCX table grids and equal column/cell widths so Apple Pages does not
  collapse inferred columns to character width.
- Fix [IF-156](../issues/IF-156.md): `extract_pdf` now honors page marked-content
  ActualText, retaining logical RTL/combining text in the generated multilingual
  sample. Bound nested spans, property references and replacement output; keep
  font-based decoding for untagged content. This is not OCR or a promise of
  lossless extraction from every PDF.
  See [Markdown Documents](../MARKDOWN_DOCUMENTS.md) for all limits and known
  format differences. Include the bundled fonts' OFL license in release and
  container packages.

## Dependency Refresh

- Integrate ZIP 9.1.2 (PR #232) without unrelated Windows/getrandom lockfile
  changes, and Comrak 0.59.0 (PR #233) with default features still disabled.
  Integrate JSON Schema 0.60.2 (PR #234) and RMCP 3.8.0 (PR #235), retaining
  the existing tempfile/getrandom resolution rather than unrelated lockfile churn.
  No stable release is published by this maintenance batch.

## Validation Resources

- Bound linked-test retention in local/CI validation by running every Cargo
  target in batches and reclaiming only reported test executables after each
  batch ([IF-158](../issues/IF-158.md)). Preserve feature/harness flags, normal
  CLI binaries, libraries and dependency caches; no tests are waived.

## Already Delivered In v1.19.2

- Integrate dependency PRs #218, #219, #221, #222, #223 and #224: serde_json
  1.0.154, SHA-2 0.11.1, tree-sitter 0.27.1 (tree-sitter-language 0.1.9),
  Redis 1.8.0, JSON Schema 0.60.1 and lopdf 0.46.0.
- Update ZIP to 9.1.0, with its LZMA 0.23.0 and typed-path 0.13.0 dependencies
  (PR #226). Existing archive admission, path and cancellation limits remain
  unchanged.
- Integrate ZIP 9.1.1 and Comrak 0.58.0 (PRs #228 and #229). Preserve unrelated
  lockfile resolutions and keep Comrak default features disabled. Check this
  feature policy structurally so dependency updates do not fail on a stale
  version assertion. Comrak fixes quadratic table-cell rendering; cover wide
  aligned tables with autocompleted cells through IronFlow's Markdown node.
- Keep PDF default features disabled and retain only the selected chrono
  feature. Preserve the existing tempfile/getrandom resolution rather than
  importing unrelated resolver churn.
- Retain Rust 1.99.1 and the v1.19.1 TLS distrust updates, container CA floors,
  and disabled SigV4a signing. No advisory ignores are added.

### Regression Coverage

- Verify object member order does not change const/enum or uniqueItems
  validation in either schema node, including inline and context schemas.
- Replace a 50 ms XLSX timing assertion with a corrupt-payload regression that
  proves declared limits are checked before decompression (IF-154).
- Check PDF dependency feature policy structurally rather than freezing an
  unrelated version string.
- Fix intermittent fresh SQLite event-store startup rejection by keeping
  index metadata verification on one refreshed connection. Retain bounded,
  redacted startup diagnostics and reap failed test children (IF-155).

The v1.19.2 dependency/SQLite changes did not introduce new workflow options or
configuration defaults. Existing v1.19.1 upgrade limitations remain documented
in [the security guide](../SECURITY.md).
