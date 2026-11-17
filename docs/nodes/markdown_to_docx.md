# `markdown_to_docx`

Render CommonMark/GFM Markdown to an editable Word (`.docx`) document with native headings, formatted runs, tables, hyperlinks and images. No external converter is needed.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | One of `input` or `source_key` | — | Literal Markdown; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `input` or `source_key` | — | Context key holding a Markdown string; its content is not interpolated. |
| `output_path` | string | Yes | — | Destination file; supports interpolation. Missing parent directories are created. |
| `output_key` | string | No | `"document"` | Prefix for the output keys. |
| `overwrite` | boolean | No | `false` | Allow replacing an existing regular file. |
| `page_size` | string | No | `"a4"` | `"a4"` or `"letter"`, portrait. |
| `margin` | number | No | `54` | Page margins in points, 18 to 144. |
| `font_size` | number | No | `11` | Body font size in points, 8 to 24. |
| `language` | string | No | `"und"` | Document language tag, such as `uk`, `ar`, `ja` or `zh-CN`. |
| `direction` | string | No | `"auto"` | `"auto"` (from each paragraph's first strong character), `"ltr"` or `"rtl"`. |
| `title` | string | No | `"IronFlow document"` | Title metadata, at most 1024 bytes. |
| `author` | string | No | `""` | Author metadata, at most 1024 bytes. |
| `images` | object | No | `{}` | Maps Markdown image references to a path, artifact URI, or artifact descriptor; at most 16. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Path of the published document. |
| `{output_key}_success` | boolean | `true` once the file is published. |
| `{output_key}_metadata` | object | `format` (`"docx"`), `bytes`, `pages` (always `null`), `blocks`, `images`. |

## Behavior

- Supplying both or neither of `input` and `source_key`, an out-of-range or wrongly typed value, or an existing `output_path` without `overwrite = true` fails the step.
- Raw HTML, links other than `http`, `https` or `mailto`, unbound image references, and images inside lists, quotes, headings or links fail the step.
- Hyperlinks are clickable. Lists and task items use literal markers in indented paragraphs, not Word numbering or checkboxes.
- Fonts are not embedded: the viewer supplies them and decides pagination, so `pages` is `null`. Install the Noto families for CJK or right-to-left text.
- Right-to-left paragraphs align right and carry explicit direction; previewers that drop these properties may still differ. Use [`markdown_to_pdf`](markdown_to_pdf.md) for fixed layout.
- The file is written atomically; a symlink or non-regular destination is refused even with `overwrite`. On Unix a new file gets `0666` masked by the umask and a replaced file keeps its mode.
- Supported syntax and the fixed limits (256 KiB source, 32 MiB output) are listed in [Markdown Documents](../MARKDOWN_DOCUMENTS.md).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` | `52428800` (50 MiB) | Maximum size of each image input; the fixed 16 MiB combined cap also applies. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | `25000000` | Maximum pixels per image; the fixed 8 million combined cap also applies. |
| `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` | `134217728` (128 MiB) | Maximum decoder memory per image. |

## Examples

### Render a report

```lua
local flow = Flow.new("markdown_docx")

flow:step("render", nodes.markdown_to_docx({
    source_key = "report_markdown",
    output_path = "output/report.docx",
    output_key = "docx",
    title = "Quarterly Report",
    author = "Report Team"
}))

return flow
```
