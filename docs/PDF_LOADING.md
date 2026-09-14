# PDF loading limits

`extract_pdf`, `pdf_metadata`, `pdf_split`, and `pdf_merge` share a bounded
`lopdf` loading policy. It applies equally to file paths and verified artifact
handles. These settings do not govern native PDFium rendering (`pdf_to_image`
and `pdf_thumbnail`), which has its own rendering limits.

| Environment variable | Default | Boundary |
|----------------------|---------|----------|
| `IRONFLOW_MAX_PDF_BYTES` | `104857600` (100 MiB) | Encoded bytes per source, checked before and during input reads |
| `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` | `67108864` (64 MiB) | Decoded bytes per object or cross-reference stream loaded by `lopdf` |
| `IRONFLOW_MAX_PDF_OBJECTS` | `250000` | Loaded document objects, checked after parsing and before downstream work |

Zero, invalid, or missing values retain the defaults. The stream ceiling is
also clamped to the platform's addressable allocation size.

## Admission and failure behavior

Load options bound object-stream and cross-reference-stream decompression,
including intermediate filter layers. Strict mode propagates unencrypted
object/object-stream load errors instead of silently skipping failed objects.
Some files previously accepted through lenient object parsing may now fail.

`lopdf` can recover from cross-reference failures and can suppress errors in
encrypted object streams. Before accepting a loaded document, IronFlow therefore
rechecks retained cross-reference streams and, for decrypted inputs, retained
object streams with the same byte ceiling. Uncompressed cross-reference streams
are checked at this stage too. An oversized retained stream fails the node before
extraction, metadata, or page output. This does not disable every upstream xref
repair path or validate every historical PDF revision. Already-written split
pages remain subject to the split node's separate partial-output contract.

Inputs readable with an empty PDF password remain supported. Files still
encrypted after loading fail; these nodes do not accept a password parameter.

The object count includes loaded container streams, cross-reference streams, and
their expanded object-stream members when retained by the parser. It is checked
after the parser returns, not before those objects are allocated. It is separate
from `IRONFLOW_MAX_PDF_MERGE_OBJECTS`, which limits the merged output graph across
sources. Each merge source must pass the loading policy even if only a small
subset of its objects will be retained in the output.

## Memory and cancellation boundaries

These are independent resource limits, not a hard process-memory ceiling or a
cumulative decompressed-byte budget. Multiple streams, parsed object overhead,
cross-reference tables, source buffers, retained merge graphs, and concurrent
tasks can coexist in memory. The object-count check is an acceptance limit, not
a preallocation guard. Use process/container memory isolation for a hard ceiling.

Page count, extraction items/output, split selection, and cumulative merge
bytes/pages/objects keep their existing limits. In particular,
`IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` still bounds text extraction and each page's
content/font-mapping decompression independently of initial loading.

Input reading, loading, checks, and downstream work run on tracked blocking
workers. Cancellation is checked around synchronous parser/decoder calls and
between retained-stream checks; it cannot interrupt an opaque call midway.
Task/run admission remains held until the worker stops.
