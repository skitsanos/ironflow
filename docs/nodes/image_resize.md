# `image_resize`

Resize a single image file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path (supports `${ctx.key}` interpolation). |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a path, artifact URI/descriptor, `{ artifact = ... }`, or an explicit `{ base64 = "..." }` object. |
| `output_path` | string | yes | — | Destination file path for the resized image. |
| `width` | number | no | — | Target width in px. Required if `height` is omitted. |
| `height` | number | no | — | Target height in px. Required if `width` is omitted. |
| `format` | string | no | inferred from `output_path` or `png` | Output format: `png`, `jpeg`, or `jpg`. |
| `output_key` | string | no | `"resized_image"` | Prefix for the generated context output keys. |

> If both `path` and `source_key` are provided, execution fails.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; decoding consumes that same rewound handle rather than a resolved store pathname.

With one dimension, the other is computed to preserve aspect ratio (rounded to
the nearest pixel, minimum 1). With both dimensions, the image is resized to
that exact size using Lanczos3, which may change its aspect ratio.

Supported source formats are BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM,
QOI, TGA, TIFF, and WebP. Output remains restricted to PNG or JPEG.

## Context Output

- `<output_key>` — output file path.
- `<output_key>_width` — output width in pixels.
- `<output_key>_height` — output height in pixels.
- `<output_key>_format` — output format (`"png"` or `"jpeg"`).
- `<output_key>_success` — `true` on success.

## Example

```lua
local flow = Flow.new("image_resize_demo")

flow:step("resize", nodes.image_resize({
    path = "examples/fixtures/ironflow-sample.png",
    output_path = "outputs/sample_front_small.png",
    width = 120
}))

flow:step("log", nodes.log({
    message = "Resized to ${ctx.resized_image_width}x${ctx.resized_image_height}"
})):depends_on("resize")

return flow
```

## Resource contract

The encoded source, decoded dimensions/pixels, decoder allocation, and computed
output dimensions are checked against `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50
MiB), `IRONFLOW_MAX_IMAGE_PIXELS` (25 million), and
`IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB). Resize admission runs
after header inspection but before pixel decoding, then is rechecked against
the decoded image before resampling. It applies equally to paths, verified
artifacts, and Base64 inputs.

The working-buffer estimate includes the retained source, the output in its
original pixel type, the `source_width * target_height * 16` byte RGBA-float
intermediate, and conservative filter-weight scratch space (including vector
growth/reallocation). The two sampling passes are budgeted by their peak, not
their sum. Same-size requests copy the source without a float intermediate.
All size arithmetic is checked; overflow or an unaddressable buffer is rejected.

For example, a 100x1 RGB image resized to 1x100 needs a 160,000-byte intermediate
even though its source and output are only 300 bytes each. A 1 KiB allocation
limit rejects it before decoding pixels or creating/overwriting the output.

This is an admission estimate for known image buffers, not a hard process-memory
ceiling. Header/codec internals, encoding scratch, encoded/Base64 input and
workflow context, allocator overhead, and concurrent tasks can require additional
memory; decoder-managed limits and encoded-input limits remain separate checks.
Decode, resize, and encode run on a tracked blocking worker; cancellation is
observed between opaque codec and transform operations, not within them.
