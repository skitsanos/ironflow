# Example fixtures

These compact, synthetic files make IronFlow's extraction and media examples
runnable from a clean checkout. They contain original test content created for
this repository and no copied documents, customer data, corporate slides, or
third-party images.

The fixtures are intentionally reused across examples:

- `ironflow-sample.pdf` — three text pages for extraction, metadata, merge,
  split, rendering, and chunking.
- `ironflow-sample.docx` — paragraphs and a small table for text and Markdown
  extraction.
- `ironflow-sample.pptx` — two editable slides for structured extraction and
  presentation reconstruction examples. The first slide carries one
  PowerPoint modern comment thread (a comment and one reply, in
  `ppt/comments/` with authors in `ppt/authors.xml`), added by hand in the
  element order PowerPoint for the web writes.
- `ironflow-sample.png` — a 640 x 480 RGB test pattern for image nodes.
- `ironflow-sample.xlsx` — a two-sheet workbook (`Parts`, `Notes`) for
  `extract_xlsx`. `Parts` carries a title row above its real header, the
  common real-world layout where the header is not row one.
- `ironflow-transcript.vtt` and `ironflow-transcript.srt` — equivalent subtitle
  transcripts for extraction, cue chunking, and service-backed examples.
- `markdown-multilingual.md` — Ukrainian, Chinese, Japanese, Korean, Hebrew,
  Arabic, combining marks, mixed-direction text and an RTL table for document generation.
- `pandoc-deck.pptx` and `pandoc-document.docx` — files written by an
  authoring tool rather than assembled by hand, extracted by
  `tests/test_tool_authored_documents.rs` (IF-222). pandoc 3.14 generated
  them from `pandoc-deck.md` and `pandoc-document.md`. The deck has speaker
  notes with a hard line break and a slide without notes; the document has
  style-based headings, direct bold, italic and strikethrough, a hard line
  break, a comment with an anchored range, a tracked insertion and deletion,
  and a table. Their text is original test content; the styles, themes, slide
  masters and layouts inside the packages come from pandoc's default
  reference documents. Regenerate them with a fixed timestamp so the bytes
  stay stable:

  ```bash
  SOURCE_DATE_EPOCH=1794700800 pandoc pandoc-deck.md -o pandoc-deck.pptx
  SOURCE_DATE_EPOCH=1794700800 pandoc pandoc-document.md -o pandoc-document.docx
  ```

Lua examples address these files through the injected `_flow_dir` context key,
so their inputs resolve relative to the flow file rather than the process's
working directory. Generated outputs must not be written back into this folder.

The fixture files are released under CC0-1.0; see
`LICENSE-CC0-1.0.txt`. `SHA256SUMS` records the reviewed byte set and is checked
by the Rust test suite. Update it only when a fixture change is intentional and
has passed format-specific render and runtime validation.
