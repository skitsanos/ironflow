# `extract_pptx`

Extract slides, speaker notes, and comments from a PowerPoint (`.pptx`) deck.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.pptx` file; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is the file path (must be a string). |
| `format` | string | no | `"text"` | Output format: `"text"` (flattened slide text), `"markdown"` (one section per slide), or `"json"` (structured IR). |
| `output_key` | string | no | `"content"` | Context key where the extracted output is stored. For `text`/`markdown` the value is a string; for `json` it is an object. |
| `metadata_key` | string | no | — | If set, deck metadata (slide count + Dublin Core fields) is stored under this context key. |
| `comments_key` | string | no | — | If set, slide comments (from `ppt/comments/comment*.xml` plus author lookup in `ppt/commentAuthors.xml`) are stored under this context key as a flat array. Comments are also attached per-slide in the JSON output. |
| `include_image_bytes` | boolean | no | `false` | When `format = "json"`, include embedded image bytes as base64 (`media_b64`) plus `mime_type` when the image relationship can be resolved. |

> Providing both `path` and `source_key` is an error. `format` accepts `"text"`, `"markdown"`, or `"json"`.

## Context Output

- `<output_key>` (default `content`) — slides as text / Markdown / JSON IR.
- `<metadata_key>` (when set) — object with `slide_count` and (when present) `title`, `author`, `subject`, `description`, `keywords`, `last_modified_by`, `created`, `modified`, `revision`, `category`.
- `<comments_key>` (when set) — flat array of `PptxComment` (see below).

## JSON output shape

```json
{
  "slides": [
    {
      "slide_index": 1,
      "title": "STIMULUS 1A",
      "elements": [
        {
          "type": "text_block",
          "placeholder": null,
          "paragraphs": [
            { "text": "20 GA Patients" },
            { "text": "Group A", "list_level": 0 },
            { "text": "Group B", "list_level": 0 }
          ]
        }
      ],
      "speaker_notes": "Moderator: probe on rationale per group.",
      "comments": [
        {
          "slide_index": 1,
          "idx": "1",
          "author_id": "0",
          "author": "Reviewer A",
          "initials": "RA",
          "date": "2026-04-02T14:20:00",
          "text": "Make sure to clarify 'group' wording."
        }
      ]
    },
    {
      "slide_index": 2,
      "title": "Patient Profile A",
      "elements": [
        {
          "type": "table",
          "rows": [
            ["Field", "Value"],
            ["Age", "75"],
            ["Diagnosis", "Bilateral GA, juxtafoveal"]
          ]
        }
      ]
    }
  ]
}
```

### Element types

- `text_block` — a non-title shape's text. `placeholder` carries the OOXML placeholder type (`subTitle`, `body`, etc.) when present. `paragraphs[]` carry `text` and optional `list_level` (0-based indent for bulleted items).
- `table` — rendered as `rows: [[string]]`.
- `image` — picture shape metadata. `alt_text`, `embed_id`, and `embedded_path` are included when available. With `include_image_bytes = true`, the JSON output also includes `media_b64` and `mime_type` for resolved embedded images.

The slide's `title` is taken from the placeholder with `type="title"` or `type="ctrTitle"`. If no title placeholder exists, the `title` field is omitted.

### Comments

The node currently parses **legacy** comments (`ppt/comments/comment*.xml`, indexed by slide number) and the matching `ppt/commentAuthors.xml`. PowerPoint's newer "modern comments" format (`ppt/modernComments/`) is not yet supported.

`slide_index` on each comment is derived from the comment file's numeric suffix (`comment3.xml` → slide 3).

## Examples

### Extract a stimulus deck as JSON

```lua
flow:step("extract_deck", nodes.extract_pptx({
    path = "${ctx.deck_path}",
    format = "json",
    include_image_bytes = true,
    output_key = "deck",
    comments_key = "deck_comments",
    metadata_key = "deck_meta"
}))

flow:step("show", nodes.log({
    message = "Deck '${ctx.deck_meta.title}' has ${ctx.deck_meta.slide_count} slides"
})):depends_on("extract_deck")
```

### Markdown for previewing

```lua
flow:step("preview", nodes.extract_pptx({
    path = "/data/stimulus.pptx",
    format = "markdown",
    output_key = "md"
}))
```

Produces one `## Slide N` section per slide with the title as an `###` header, body paragraphs (bullets if list-leveled), tables as Markdown pipe tables, and speaker notes appended.
