# Example fixtures

These compact, synthetic files make IronFlow's extraction and media examples
runnable from a clean checkout. They contain original test content created for
this repository and no copied documents, customer data, corporate slides, or
third-party images.

The six files are intentionally reused across examples:

- `ironflow-sample.pdf` — three text pages for extraction, metadata, merge,
  split, rendering, and chunking.
- `ironflow-sample.docx` — paragraphs and a small table for text and Markdown
  extraction.
- `ironflow-sample.pptx` — two editable slides for structured extraction and
  presentation reconstruction examples.
- `ironflow-sample.png` — a 640 x 480 RGB test pattern for image nodes.
- `ironflow-transcript.vtt` and `ironflow-transcript.srt` — equivalent subtitle
  transcripts for extraction, cue chunking, and service-backed examples.

Lua examples address these files through the injected `_flow_dir` context key,
so their inputs resolve relative to the flow file rather than the process's
working directory. Generated outputs must not be written back into this folder.

The fixture files are released under CC0-1.0; see
`LICENSE-CC0-1.0.txt`. `SHA256SUMS` records the reviewed byte set and is checked
by the Rust test suite. Update it only when a fixture change is intentional and
has passed format-specific render and runtime validation.
