# `image_rotate`

Rotate a single image by 90-degree increments.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path, artifact URI/descriptor, or explicit Base64 object |
| `output_path` | string | yes | — | Destination image path |
| `angle` | number | no | `90` | One of `90`, `180`, `270` |
| `format` | string | no | inferred / `png` | `png` or `jpeg`/`jpg` |
| `output_key` | string | no | `"rotated_image"` | Prefix for output values |

Artifact inputs are opened and SHA-256 verified inside the tracked blocking
worker; decoding consumes that same rewound handle rather than a resolved store
pathname.

Supported source formats are BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM,
QOI, TGA, TIFF, and WebP. Output remains restricted to PNG or JPEG.

## Context Output

- `<output_key>` — output file path
- `<output_key>_angle` — angle used
- `<output_key>_width` / `<output_key>_height` — output dimensions
- `<output_key>_source_width` / `<output_key>_source_height`
- `<output_key>_format`
- `<output_key>_success`

## Example

```lua
local flow = Flow.new("image_rotate_demo")

flow:step("rotate", nodes.image_rotate({
    path = "examples/fixtures/ironflow-sample.png",
    angle = 90,
    output_path = "output/sample_front_rotated.png",
    output_key = "rotated"
}))

flow:step("log", nodes.log({
    message = "Rotated image: ${ctx.rotated_width}x${ctx.rotated_height}"
})):depends_on("rotate")

return flow
```

## Destination contract

`output_path` is published through the same rooted, atomic writer as
[`write_file`](write_file.md), inside the tracked blocking worker: the rotated
image is encoded into a staged file beside the destination, flushed and
synchronized, then renamed over it.

- The destination directory must already exist. The node creates no
  directories and fails with `image_rotate: destination directory '<dir>' does
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

The encoded source, decoded pixels/allocation, and rotated output buffer are
checked against `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB),
`IRONFLOW_MAX_IMAGE_PIXELS` (25 million), and
`IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB). The working estimate
includes retained source plus rotated output. Decode, rotation, and
encode run on a tracked blocking worker with cancellation checkpoints between
opaque operations.
