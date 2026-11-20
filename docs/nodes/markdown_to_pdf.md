# `markdown_to_pdf`

Render CommonMark/GFM Markdown to a searchable, paginated PDF file with embedded fonts, tables and bound raster images. No external renderer is needed.

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
| `{output_key}` | string | Path of the published PDF. |
| `{output_key}_success` | boolean | `true` once the file is published. |
| `{output_key}_metadata` | object | `format` (`"pdf"`), `bytes`, `pages`, `blocks`, `images`. |

## Behavior

- Supplying both or neither of `input` and `source_key`, an out-of-range or wrongly typed value, or an existing `output_path` without `overwrite = true` fails the step.
- Raw HTML, links other than `http`, `https` or `mailto`, unbound image references, and images inside lists, quotes, headings or links fail the step.
- Links show their URL as text; they are not clickable. Tables repeat their header across pages; a table too wide for the page fails the step.
- Chinese, Japanese and Korean need a `cjk-fonts` build (`-full` binaries, the container image) or fonts in `IRONFLOW_FONT_DIR`. A character no font covers fails the step.
- The file is written atomically; a symlink or non-regular destination is refused even with `overwrite`. On Unix a new file gets `0666` masked by the umask and a replaced file keeps its mode.
- `extract_pdf` reads the logical text back, including right-to-left scripts. No PDF/A or PDF/UA conformance is claimed.
- Supported syntax and the fixed limits (256 KiB source, 250 pages, 32 MiB output) are listed in [Markdown Documents](../MARKDOWN_DOCUMENTS.md).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` | `52428800` (50 MiB) | Maximum size of each image input; the fixed 16 MiB combined cap also applies. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | `25000000` | Maximum pixels per image; the fixed 8 million combined cap also applies. |
| `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` | `134217728` (128 MiB) | Maximum decoder memory per image. |

## Examples

### Render a report

```lua
local flow = Flow.new("markdown_pdf")

flow:step("render", nodes.markdown_to_pdf({
    source_key = "report_markdown",
    output_path = "output/report.pdf",
    output_key = "pdf",
    page_size = "a4",
    margin = 54,
    font_size = 11,
    title = "Quarterly Report"
}))

return flow
```

Use [`markdown_to_docx`](markdown_to_docx.md) for an editable Word document.
