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
`IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB) before resize allocation;
the allocation check includes the retained source and computed output buffer.
Decode, resize, and encode run on a tracked blocking worker; cancellation is
observed between opaque codec and transform operations.
