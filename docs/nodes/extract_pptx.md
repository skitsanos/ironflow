# `extract_pptx`

Extract slide text, tables, images, speaker notes, and comments from a PowerPoint (`.pptx`) deck as text, Markdown, or JSON.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path`/`source_key` | — | Path to the `.pptx` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path`/`source_key` | — | Context key holding a file path, artifact URI, or artifact descriptor; the stored path is not interpolated. |
| `format` | string | No | `"text"` | `"text"`, `"markdown"`, or `"json"`; any other value fails the step. |
| `output_key` | string | No | `"content"` | Context key for the result (string for text/Markdown, object for JSON). |
| `metadata_key` | string | No | — | When set, store deck properties and `slide_count` under this key. |
| `comments_key` | string | No | — | When set, also store all slide comments as one flat array under this key. |
| `media_mode` | string | No | `"none"` | `"none"` reports image metadata only; `"artifact"` stores each embedded image in the artifact store. `"artifact"` requires `format = "json"`. |
| `include_image_bytes` | boolean | No | `false` | Deprecated; `true` fails the step with a pointer to `media_mode = "artifact"`. |

`path` and `source_key` are mutually exclusive. `output_key`, `metadata_key` and `comments_key` must name different keys. String parameters of the wrong type fail the step.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string or object | Slides as text, Markdown, or JSON. |
| `{metadata_key}` | object | `slide_count` plus available `title`, `author`, `subject`, `description`, `keywords`, `last_modified_by`, `created`, `modified`, `revision`, `category`. |
| `{comments_key}` | array | Every comment in presentation order, each with its `slide_index`. |

## Output Shape

```json
{
  "slides": [
    {
      "slide_index": 1,
      "title": "STIMULUS 1A",
      "elements": [
        { "type": "text_block", "placeholder": "body",
          "paragraphs": [ { "text": "First point", "list_level": 0 } ] },
        { "type": "table", "rows": [["Field", "Value"], ["Age", "75"]] },
        { "type": "image", "alt_text": "Clinical scan", "embed_id": "rId3",
          "embedded_path": "ppt/media/image1.png",
          "artifact": { "artifact_uri": "artifact://sha256/0123…", "sha256": "0123…",
                        "size_bytes": 184322, "mime_type": "image/png" } }
      ],
      "speaker_notes": "Moderator: probe on rationale per group.",
      "comments": [
        { "slide_index": 1, "id": "{6F1A…01}", "author_id": "{6F1A…A1}",
          "author": "Reviewer A", "initials": "RA", "date": "2026-11-17T09:00:00.000",
          "status": "resolved", "text": "Check the chart",
          "replies": [ { "id": "{6F1A…02}", "author": "Reviewer B", "initials": "RB",
                         "date": "2026-11-17T09:05:00.000", "text": "Updated." } ] }
      ]
    },
    { "slide_index": 2, "hidden": true, "elements": [] }
  ]
}
```

- `slide_index` is the one-based position in the slide list. `hidden: true` marks a slide hidden from the slide show; shown slides have no `hidden` field.
- `title` comes from the `title` or `ctrTitle` placeholder. `title`, `speaker_notes` and `comments` are omitted when empty.
- `placeholder` is the placeholder type when the shape has one. `list_level` (0-based) is present only on bulleted or numbered paragraphs, including bullets inherited from the slide layout and master.
- `embedded_path` is present when the image resolves to a part in the package; `artifact` only with `media_mode = "artifact"`.
- Legacy comments carry `idx` and no `id`, `status` or `replies`. Modern comments carry `id`, `status` when written (`active`, `resolved`, `closed`), and `replies` when they have any; replies have the comment fields without `slide_index`, `idx` and `replies`.

## Behavior

- Hidden slides are included and counted; only JSON marks them.
- Footer, date, slide-number and header placeholders are left out of slide text and speaker notes in every format. Field values such as slide numbers are skipped.
- Speaker notes are the body placeholder of the notes slide.
- A line break is a newline in paragraph text and notes, and a space in titles and table cells.
- Comments appear in every format. A slide with modern comments reports only those, even if it also keeps a legacy copy. An author missing from the author list leaves `author_id` only; text and Markdown show `?`.
- Text writes `===== SLIDE N =====`, `--- NOTES ---`, and comments as `[COMMENT by <author> @ <date> (<status>)]: <text>` with replies as `  [REPLY by ...]`.
- Markdown writes `## Slide N`, the title as `###`, list items, pipe tables, `*(image)*`, `**Speaker notes:**`, and comments as `> 💬 **<author>** (<date>, <status>): <text>` with replies as `>   ↳ ...`.
- A malformed part, a DTD, a missing slide or linked notes/comments part, or an external or escaping relationship fails the step. Images already stored as artifacts stay stored when a later check fails.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | `52428800` | Maximum input file size. |
| `IRONFLOW_MAX_ZIP_ENTRIES` | `10000` | Maximum entries in the package. |
| `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | `536870912` | Maximum total declared uncompressed size. |
| `IRONFLOW_MAX_EXTRACT_ITEMS` | `250000` | XML events and parsed items per call. |
| `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` | `52428800` | Maximum serialized result, content plus metadata and comments. |

Artifact images are written under `IRONFLOW_ARTIFACT_DIR` (see [CLI_REFERENCE.md](../CLI_REFERENCE.md)); archive validation rules are in [SECURITY.md](../SECURITY.md).

## Examples

### Extract a stimulus deck as JSON

```lua
flow:step("extract_deck", nodes.extract_pptx({
    path = "${ctx.deck_path}",
    format = "json",
    media_mode = "artifact",
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
