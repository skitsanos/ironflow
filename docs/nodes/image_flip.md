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

Supported source formats are BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM,
QOI, TGA, TIFF, and WebP. Output remains restricted to PNG or JPEG.

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

## Destination contract

`output_path` is published through the same rooted, atomic writer as
[`write_file`](write_file.md), inside the tracked blocking worker: the flipped
image is encoded into a staged file beside the destination, flushed and
synchronized, then renamed over it.

- The destination directory must already exist. The node creates no
  directories and fails with `image_flip: destination directory '<dir>' does
  not exist`. Directory aliases, including macOS `/tmp`, are resolved first.
- A destination that is a symlink (dangling or not), FIFO, device or directory
  is refused and its target is left untouched. An existing regular file is
  replaced.
- An encoder or write failure, an expired step deadline or cancellation
  removes the staged file and leaves an existing destination unchanged.
- On Unix the published file has mode `0600` (owner read and write only),
  whatever the umask and whatever mode the replaced file had; widen it in a
  later step when another user must read it. Other platforms use their native
  permission semantics. This is the staged writer's mode, under review in
  [IF-185](../issues/IF-185.md).
- Unix pins the resolved directory and stages relative to that handle. Other
  platforms revalidate the observed path immediately before an atomic
  replacement and cannot close a hostile parent-directory swap race; protect
  destination trees from same-identity mutation.

## Resource contract

Image headers and decoded byte requirements are checked before allocation using
`IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB), `IRONFLOW_MAX_IMAGE_PIXELS` (25
million), and `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB); the latter
includes retained source plus flip output. Decode,
flip, and encode run on a tracked blocking worker with cancellation checkpoints
between opaque image-library operations.
