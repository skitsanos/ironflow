# `image_flip`

Flip a single image horizontally or vertically.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path, artifact URI/descriptor, or explicit Base64 object |
| `output_path` | string | yes | — | Destination image path |
| `direction` | string | no | `"horizontal"` | `horizontal`, `vertical`, or `both` |
| `format` | string | no | inferred / `png` | `png` or `jpeg`/`jpg` |
| `output_key` | string | no | `"flipped_image"` | Prefix for output values |

Artifact inputs are opened and SHA-256 verified inside the tracked blocking
worker; decoding consumes that same rewound handle rather than a resolved store
pathname.

## Context Output

- `<output_key>` — output file path
- `<output_key>_direction` — chosen direction
- `<output_key>_width` / `<output_key>_height`
- `<output_key>_format`
- `<output_key>_success`

## Example

```lua
local flow = Flow.new("image_flip_demo")

flow:step("flip", nodes.image_flip({
    path = "examples/fixtures/ironflow-sample.png",
    direction = "vertical",
    output_path = "output/sample_front_flip.png",
    output_key = "flipped"
}))

flow:step("log", nodes.log({
    message = "Flipped image file: ${ctx.flipped}"
})):depends_on("flip")

return flow
```

## Resource contract

Image headers and decoded byte requirements are checked before allocation using
`IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB), `IRONFLOW_MAX_IMAGE_PIXELS` (25
million), and `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB); the latter
includes retained source plus flip output. Decode,
flip, and encode run on a tracked blocking worker with cancellation checkpoints
between opaque image-library operations.
