# Markdown Documents

`markdown_to_pdf` and `markdown_to_docx` generate documents locally from the same
CommonMark/GFM source. No external converter, Python runtime, browser, network
service, or Pdfium library is required for generation. Both nodes are available
with default features and with `--no-default-features`.

## Parameters

| Parameter | Type | Default | Contract |
|-----------|------|---------|----------|
| `input` | string | required unless `source_key` | Literal Markdown; one-pass `${ctx...}` interpolation, with string-valued substitutions only. |
| `source_key` | string | required unless `input` | Context key containing a Markdown string. Its content is not interpolated, which preserves literal placeholders and code. |
| `output_path` | string | required | Destination file, with bounded string interpolation. Missing parents are created. |
| `output_key` | string | `document` | Prefix for the output path, success flag and metadata. |
| `overwrite` | boolean | `false` | Explicitly allow atomic replacement of an existing regular file. |
| `page_size` | string | `a4` | `a4` or `letter`, portrait. |
| `margin` | number | `54` | All four page margins, in points; range 18 to 144. |
| `font_size` | number | `11` | Body font size, in points; range 8 to 24. |
| `language` | string | `und` | Document language tag, for example `uk`, `ar`, `he`, `ja`, `ko`, `zh-CN` or `zh-TW`. `und` leaves the primary language unspecified and permits paragraph-level CJK locale inference. |
| `direction` | string | `auto` | `auto` uses each paragraph's first strong character; `ltr` or `rtl` explicitly sets its base direction. Mixed-direction runs retain Unicode bidi ordering. Code blocks use an LTR base. |
| `title` | string | `IronFlow document` | Document title metadata, at most 1024 UTF-8 bytes after interpolation. |
| `author` | string | empty string | Author metadata, at most 1024 UTF-8 bytes after interpolation. |
| `images` | object | empty object | Map Markdown image references to explicit local paths, canonical artifact URIs, or artifact descriptors. Path strings support bounded interpolation. |

Exactly one of `input` and `source_key` must be supplied. Invalid present values
are errors, not requests to use a default. Numeric and boolean parameters also
accept the normal IronFlow string/interpolation forms. `output_path` is not
inferred from the requested format; callers should choose `.pdf` or `.docx`.

## Content And Layout

- Headings 1 through 6, paragraphs, bold, italic and strikethrough are retained.
- Ordered lists retain their starting number; nested lists, task markers and
  blockquotes use indented paragraphs with literal markers. DOCX does not yet
  emit native Word numbering definitions or interactive task controls.
- Fenced/indented code and inline code use monospace text. PDF expands tabs to
  four spaces and wraps long lines; DOCX retains tabs and explicit line breaks.
- GFM tables become real document tables. A header repeats when a table crosses
  pages. DOCX uses explicit, equal-width grid columns and cell widths within the
  page margins, so importers do not have to infer the table grid. Markdown
  column-alignment hints are not currently applied. PDF rejects
  tables whose columns cannot fit a conservative minimum width under the chosen
  page, font and margins; reduce columns/font/margins or use DOCX.
- DOCX hyperlinks are clickable. PDF links show the label followed by the target
  URL as searchable text; they are not currently clickable annotations. Only
  `https`, `http` and `mailto` links without credentials are accepted, up to
  2048 UTF-8 bytes per target.
- Raster images are scaled proportionally to fit the content area, at most
  0.75 points per input pixel (96 DPI), without upscaling. Image
  references must have explicit bindings; no URLs are fetched and no paths are
  implicitly resolved from Markdown. Animated sources use their first frame.
- Images within lists, quotes, headings, links or other inline formatting are
  rejected. Put an image in a top-level paragraph. Text beside a top-level image
  is emitted as separate paragraphs, preserving reading order.
- Raw HTML, unsupported AST constructs and XML-incompatible control characters
  are rejected. Escape literal angle brackets when needed. There is no CSS,
  JavaScript, template execution, SVG rendering or remote asset loading.

PDF uses Parley for shaping, Unicode bidi ordering, fallback and line breaking,
with Krilla for output. Bundled Noto Serif/Mono, Arabic and Hebrew fonts cover
Ukrainian, Hebrew and Arabic samples, including mixed English/RTL text and
combining marks. The pan-CJK font (about 29 MiB) is embedded only in builds
with the `cjk-fonts` feature: the `-full` release binaries and the container
image. Other builds load CJK, or any additional, fonts from the directory named
by `IRONFLOW_FONT_DIR`: every `.ttf`, `.otf` and `.ttc` file there (at most 64
files of at most 64 MiB each) is registered once per process after the bundled
fonts, and its families become fallbacks for the CJK scripts. CJK text without
any CJK font fails before layout with an error naming the feature and the
variable. The font registry does not discover system fonts or fetch anything
remotely. Characters outside the available coverage still fail explicitly; this
is not a guarantee for every language or emoji.

PDF embeds subset fonts and line-level logical `/ActualText` spans.
IronFlow's `extract_pdf` honors these page spans with bounded property resolution,
preserving logical Unicode for the tested multilingual round trip ([IF-156](issues/IF-156.md)).
Covered glyphs are replaced once, including nested spans; ordinary untagged
content retains font-based decoding. This does not promise that untagged PDFs,
structure-tree-only replacements, or every external PDF-to-text tool will yield
logical RTL text or byte-identical source. Readers can apply their own bidi and
geometric reading-order heuristics. Retain the source Markdown when it is
available. No PDF/A or PDF/UA conformance is claimed.

DOCX emits explicit paragraph/run direction, script-specific font/language
settings, complex-script formatting and RTL table ordering. Body text has an
explicit default Normal style rather than inheriting the first heading style.
LTR paragraphs and code are left-aligned. RTL paragraphs use logical start
alignment through OOXML AlternateContent for modern readers, with physical
right alignment in the fallback and the paragraph style for simpler importers.
Heading styles retain the same direction-specific fallback. A preview that
discards both styles and compatibility markup can still lose alignment;
the PDF is the fixed-layout reference. Fonts are **not
embedded in DOCX**: the consuming Word/LibreOffice installation must have the
corresponding Noto families or suitable fallback fonts. A viewer without CJK
fonts can show missing characters even though the Unicode text is retained.
Pagination remains viewer-dependent. The two formats do not promise identical
line wrapping. See [font provenance and licenses](../assets/fonts/README.md).
The tested Codex preview renderer preserves the paragraph-alignment fallbacks
but does not mirror RTL table column order; Pages and LibreOffice do. Keep the
standard stored cell order rather than reversing cells to suit one previewer.
See [IF-157](issues/IF-157.md) for the importer regressions and validation scope.

The [multilingual example](../examples/07-advanced/markdown_multilingual.lua)
uses a repository-owned fixture covering these languages and mixed directions.
It also runs `extract_pdf` on the generated PDF, exposing logical text as
`pdf_text` and metadata as `pdf_text_metadata`.

## Limits And Durability

These first-version bounds are fixed, not environment overrides:

| Boundary | Maximum |
|----------|---------|
| Markdown source / interpolated source | 262144 UTF-8 bytes |
| Parsed AST | 20000 nodes, nesting depth 32 |
| PDF expanded layout text, including URL labels and repeated table headers | 1 MiB |
| Each table | 32 columns, 2000 rows including the header |
| Image bindings | 16 |
| Unique image inputs combined | 16 MiB encoded, 8 million decoded pixels |
| Normalized PNG images combined | 16 MiB |
| Each generated DOCX XML part | 16 MiB |
| Generated PDF | 250 pages |
| Published output | 32 MiB |
| Simultaneous generation workers per process | 2, shared by both formats |

Image inputs additionally obey the existing `IRONFLOW_MAX_IMAGE_ENCODED_BYTES`,
`IRONFLOW_MAX_IMAGE_PIXELS` and `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES`
settings. Artifact identity is verified through the existing file-source path.
Unused bindings are not opened. SVG is not a supported raster input.

Parsing, image decoding, layout, ZIP creation and writes run on tracked blocking
workers. Admission permits remain held until physical workers exit. Parsing and
the adapters have cooperative cancellation/deadline checkpoints; external
layout, font subsetting and final PDF serialization are bounded by admitted
content, but are not interruptible midway through a library call. The AST limit
is checked after parsing; the PDF page limit is enforced when allocating pages.
Neither is a hard memory or CPU ceiling. The output-byte limit is a publication bound, not a peak-memory
guarantee. Use process isolation when a hard resource ceiling is required.

Output is staged beside the destination, synchronized and atomically committed.
Failures preserve an existing destination and remove the staging file. Final
symlinks and non-regular destination files are refused even with `overwrite`.
Configured parent-directory aliases are resolved before staging; on Unix the
resolved directory is pinned. This is durability protection, not a filesystem
sandbox. Newly created parent directories may remain after failure.

Each node commits its own file independently. A flow generating both formats is
not a two-file transaction: one valid output can remain if the other node fails.

## Context Output

- `<output_key>`: published file path string.
- `<output_key>_success`: `true`, only after commit.
- `<output_key>_metadata`: `format`, `bytes`, `pages`, `blocks`, `images`.

`pages` is the PDF page count, or JSON null for DOCX because the consuming viewer
decides pagination. `blocks` counts shared-model blocks after list/quote
flattening; `images` counts distinct image references used, not occurrences.

## Both Formats

```lua
local flow = Flow.new("documents")

flow:step("pdf", nodes.markdown_to_pdf({
    source_key = "report_markdown",
    output_path = "output/report.pdf",
    output_key = "pdf",
    title = "Quarterly Report"
}))

flow:step("docx", nodes.markdown_to_docx({
    source_key = "report_markdown",
    output_path = "output/report.docx",
    output_key = "docx",
    title = "Quarterly Report"
}))

return flow
```

For `![Logo](logo)`, supply `images = { logo = "assets/logo.png" }` or bind a
canonical artifact URI/descriptor instead. The runnable
[example](../examples/07-advanced/markdown_documents.lua) generates both formats
with a repository-owned raster fixture and no external services.
