# `extract_pptx`

Extract slides, speaker notes, and comments from a PowerPoint (`.pptx`) deck.

## Presentation Order

`ppt/presentation.xml` and `ppt/_rels/presentation.xml.rels` are required. The
presentation's slide list determines membership and order in every output
format. `slide_index` is the one-based position in that list, not a part filename
suffix or the presentation's internal slide ID. Hidden slides that remain in the
list are included. Orphan slide, note, and comment parts are not extracted.

Slide parts may be renamed or stored in other package directories. Notes and
legacy comments are followed through each slide's actual relationship part;
comment authors are followed through the presentation's `commentAuthors`
relationship. Image relationships are also relative to the actual slide part.
An absent notes/comments relationship means no associated content, even if a
similarly numbered file exists in the ZIP.

Missing/empty manifests, duplicate slide IDs, relationship IDs or normalized
slide targets, missing referenced parts, wrong slide relationship types, and
external slide/notes/comments/authors targets are errors. Multiple notes,
comments, or author-list relationships on their respective source are rejected
as ambiguous. There is no filename-based fallback. Slide, note, comment, and
author-list references are checked before slide media is published; later
content or output errors still do not roll back already published artifacts.

Presentation and slide-reference namespaces accept transitional and strict OOXML
URIs with arbitrary prefixes. Relationship XML accepts the OPC namespace and
legacy unqualified elements, but not foreign namespaces. Presentation nesting is
capped at 128 elements. This is structural extraction, not slide rendering or a
complete OOXML schema validator; visual reading order within a slide and modern
comments are outside this contract.

## Text Fidelity

Slide text, table cells, speaker notes, metadata and comments decode built-in XML
entities and numeric references once and preserve literal CDATA. Adjacent events
and runs do not introduce spaces or newlines; speaker-note paragraphs are
separated by newlines. Metadata outer whitespace is trimmed after accumulation.
Picture descriptions, embed IDs, list levels and comment attributes use the
part's declared XML-version normalization (1.0 by default).

Unknown/invalid text references and DTDs return errors; no custom or external
entities are expanded. Decoding and retained attribute values are admitted
before allocation against existing output budgets. Table-text copies, generated
separators and comment fan-out are charged. ZIP byte/entry, XML event/item,
deadline, and final serialized-output checks are unchanged.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.pptx` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. |
| `format` | string | no | `"text"` | Output format: `"text"` (flattened slide text), `"markdown"` (one section per slide), or `"json"` (structured IR). |
| `output_key` | string | no | `"content"` | Context key where the extracted output is stored. For `text`/`markdown` the value is a string; for `json` it is an object. |
| `metadata_key` | string | no | — | If set, deck metadata (slide count + Dublin Core fields) is stored under this context key. |
| `comments_key` | string | no | — | If set, relationship-linked legacy slide comments and their author lookup are stored under this context key as a flat array in presentation order. Comments are also attached per-slide in the JSON output. |
| `media_mode` | string | no | `"none"` | Embedded-media handling: `"none"` keeps relationship metadata only; `"artifact"` streams each resolved image to the disk-backed artifact store and adds an `artifact` descriptor. Artifact mode requires `format = "json"`. |
| `include_image_bytes` | boolean | no | `false` | Deprecated compatibility input. `false` is accepted; `true` is rejected with a migration error because extraction no longer materializes inline Base64 media. |

> Providing both `path` and `source_key` is an error. `format` accepts `"text"`, `"markdown"`, or `"json"`.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; OOXML parsing consumes that same rewound handle rather than a resolved store pathname.
> Present `format`, `media_mode`, `output_key`, `metadata_key`, and `comments_key` values must be strings. The deprecated `include_image_bytes` input accepts a native boolean or an interpolated boolean spelling (`true`/`false`, `yes`/`no`, `on`/`off`, or `1`/`0`, case-insensitive). Present values of the wrong type are rejected instead of being treated as absent.
> `output_key`, `metadata_key`, and `comments_key` must be pairwise distinct when the optional keys are set. `media_mode = "artifact"` is rejected unless `format = "json"`.

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
          "type": "image",
          "alt_text": "Clinical scan",
          "embed_id": "rId3",
          "embedded_path": "ppt/media/image1.png",
          "artifact": {
            "artifact_uri": "artifact://sha256/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "size_bytes": 184322,
            "mime_type": "image/png"
          }
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

- `text_block` — a non-title shape's text. `placeholder` carries the OOXML placeholder type (`subTitle`, `body`, etc.) when present and is omitted otherwise. `paragraphs[]` carry `text` and optional `list_level` (0-based indent for bulleted items).
- `table` — rendered as `rows: [[string]]`.
- `image` — picture shape metadata. `alt_text` and `embed_id` are included when available. `embedded_path` is included only when that ID resolves through an internal OOXML image relationship. With `media_mode = "artifact"`, a resolved embedded image also has an `artifact` object containing its canonical URI, SHA-256 digest, decoded byte size, and MIME type. Binary bytes are not placed in `NodeOutput`.

The slide's `title` is taken from the placeholder with `type="title"` or `type="ctrTitle"`. If no title placeholder exists, the `title` field is omitted.

### Comments

The node currently parses **legacy** comments through `comments` relationships
and authors through the presentation's `commentAuthors` relationship. Conventional
part names such as `ppt/comments/comment3.xml` and `ppt/commentAuthors.xml` are
not required. PowerPoint's newer "modern comments" format is not yet supported.

`slide_index` on each comment is the referring slide's one-based presentation
position, in both the flat comments array and the per-slide JSON output.

## Resource and cancellation contract

- The input must be a regular file. Its raw size and central directory are
  bounded by `IRONFLOW_MAX_FILE_BYTES` (default `52428800`, 50 MiB) before the
  ZIP library allocates archive metadata. On Unix and Windows, IronFlow also
  refuses to follow a final path-component symlink/reparse point; other platforms enforce the
  opened-handle regular-file check.
- The EOCD/ZIP64 preflight validates central-directory bounds and
  `IRONFLOW_MAX_ZIP_ENTRIES` (default `10000`). IronFlow then rejects duplicate
  part names, symlink or special-file parts, and a cumulative declared
  uncompressed size above `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` (default
  `536870912`, 512 MiB). These archive-wide admission checks include orphan parts,
  even though their XML and media are not read during extraction.
- XML parts are parsed directly from bounded ZIP-entry readers; IronFlow does
  not first materialize a complete part as a byte vector or string. Each part
  is capped by the smaller of the remaining cumulative ZIP budget and the
  extraction-output budget, and actual decoded bytes accumulate across every
  part read. All read, UTF-8, and XML parse errors in the manifest and consumed
  slide, notes, relationship, comment, author, and core-properties parts propagate.
  Unlinked notes/comments/authors and missing optional metadata remain absent or
  empty; linked notes/comments/authors must exist. Errors and limit
  breaches never become partial output. `quick_xml` still buffers its current
  event, so one unusually large text or CDATA token can require memory
  proportional to that token, but not to the complete XML part.
- `IRONFLOW_MAX_EXTRACT_ITEMS` (default `250000`) is shared across presentation
  membership, presentation/slide/relationship/content-type XML events,
  content-type definitions, slides, elements, paragraphs, table rows and cells,
  relationships, comments and authors, metadata fields, and embedded media
  occurrences. Content-type work occurs only when artifact media is requested.
- `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` (default `52428800`, 50 MiB) bounds
  retained fields and formatting work and, authoritatively, the complete
  serialized `NodeOutput`. That final result includes the slide output,
  metadata, and the optional flat comments copy. This is a logical result
  limit, not a process-RSS limit and not the later
  `IRONFLOW_MAX_TASK_OUTPUT_BYTES` persistence limit.
- Embedded media is decoded only when `format = "json"` and
  `media_mode = "artifact"`. It is copied from the ZIP entry to a private
  artifact-store staging file in bounded chunks while cancellation, cumulative
  decoded ZIP bytes, and the per-part limit are enforced. The bytes are hashed
  during that copy and published atomically under `IRONFLOW_ARTIFACT_DIR`
  (default `data/artifacts`) as
  `sha256/<digest>`. Repeated references to the same normalized package part
  reuse one descriptor and one stored file. Relationship attributes are XML
  decoded before use, duplicate relationship IDs are rejected, and only the
  transitional or strict OOXML `image` relationship type is eligible for
  `embedded_path` or artifact publication. For image elements, non-image
  relationships and external targets are ignored and never opened or fetched.
  In artifact mode, a referenced internal image whose media part is missing is
  an error; metadata-only mode does not open the image bytes.
  Relationship elements must contain non-empty `Id`, `Type`, and `Target`
  attributes; an unknown `TargetMode` is rejected instead of being treated as
  internal. Internal targets resolve relative to their actual source part; a
  single leading `/` denotes the package root, never the host filesystem.
  Paths cannot escape the package root or contain URI schemes, backslashes,
  ASCII control characters, queries, fragments, empty segments, or terminal
  dot segments. Case, Unicode, and percent-looking sequences are preserved;
  targets are not URL-decoded before ZIP member lookup. Relationship fields,
  slide identities, and normalized retained paths share the output budget.
- In artifact mode, the descriptor MIME type comes from the matching
  `[Content_Types].xml` override or extension default when one is declared;
  both lookups follow the package's ASCII case-insensitive matching rules and
  an override takes precedence.
  IronFlow falls back to its built-in image-extension mapping when the package
  omits a matching declaration, and to `application/octet-stream` when neither
  source recognizes the extension. MIME content is not inferred from the
  binary payload. A present malformed content-types part is an extraction
  error.
- Artifact descriptors are small JSON values; the media itself is never
  Base64-encoded or retained in the extraction model. Artifacts are immutable
  and content-addressed. The local backend does not automatically expire or
  garbage-collect them, so operators own retention. A recovered or multi-host
  workflow must mount the same artifact directory on every worker; Redis or
  PostgreSQL context persistence does not copy local artifact files. Artifact
  publication is not rolled back if a later slide or output check fails, so a
  retention sweep must also reclaim unreferenced files.
- ZIP and XML work runs on a tracked blocking worker. Archive reads and parser
  loops check cancellation and the step/run deadline between chunks and XML
  events; synchronous ZIP-library construction cannot be interrupted inside
  the call and is bracketed by checkpoints. Task and run admission remain
  occupied until the physical worker stops.

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

Produces one `## Slide N` section per slide with the title as an `###` header, body paragraphs (bullets if list-leveled), tables as Markdown pipe tables, and speaker notes appended.
