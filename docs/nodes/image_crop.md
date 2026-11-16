# `image_crop`

Crop a single image file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path (supports `${ctx.key}` interpolation). |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a path, artifact URI/descriptor, `{ artifact = ... }`, or an explicit `{ base64 = "..." }` object. A stored path is used as it is and is not interpolated. |
| `output_path` | string | yes | — | Destination file path for the cropped image. |
| `x` | number | no | `0` | Left offset in pixels. |
| `y` | number | no | `0` | Top offset in pixels. |
| `crop_width` | number | no | alias: `width` | Crop width in pixels. |
| `crop_height` | number | no | alias: `height` | Crop height in pixels. |
| `format` | string | no | inferred from `output_path` or `png` | Output format: `png`, `jpeg`, or `jpg`. |
| `output_key` | string | no | `"cropped_image"` | Prefix for the generated context output keys. |

> If both `path` and `source_key` are provided, execution fails.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; decoding consumes that same rewound handle rather than a resolved store pathname.

Supported source formats are BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM,
QOI, TGA, TIFF, and WebP. Output remains restricted to PNG or JPEG.

## Context Output

- `<output_key>` — output file path.
- `<output_key>_width` — crop width.
- `<output_key>_height` — crop height.
- `<output_key>_x` — x offset used.
- `<output_key>_y` — y offset used.
- `<output_key>_format` — output format (`"png"` or `"jpeg"`).
- `<output_key>_success` — `true` on success.

## Example

```lua
local flow = Flow.new("image_crop_demo")

flow:step("crop", nodes.image_crop({
    path = "examples/fixtures/ironflow-sample.png",
    output_path = "outputs/sample_front_cropped.png",
    x = 10,
    y = 8,
    crop_width = 120,
    crop_height = 80
}))

flow:step("log", nodes.log({
    message = "Cropped image: ${ctx.cropped_image_width}x${ctx.cropped_image_height}"
})):depends_on("crop")

return flow
```

## Destination contract

`output_path` is published through the same rooted, atomic writer as
[`write_file`](write_file.md), inside the tracked blocking worker: the cropped
image is encoded into a staged file beside the destination, flushed and
synchronized, then renamed over it.

- The destination directory must already exist. The node creates no
  directories and fails with `image_crop: destination directory '<dir>' does
  not exist`. Directory aliases, including macOS `/tmp`, are resolved first.
- A destination that is a symlink (dangling or not), FIFO, device or directory
  is refused and its target is left untouched. An existing regular file is
  replaced.
- An encoder or write failure, an expired step deadline or cancellation
  removes the staged file and leaves an existing destination unchanged.
- On Unix the published file has the mode a plain write would have left: a
  new file gets `0666` masked by the process umask (`0644` under umask `022`)
  and a replaced file keeps its permission bits, without any setuid, setgid
  or sticky bit. The staged file is mode `0600` until it is published. Other
  platforms use their native permission semantics.
- Unix pins the resolved directory and stages relative to that handle. Other
  platforms revalidate the observed path immediately before an atomic
  replacement and cannot close a hostile parent-directory swap race; protect
  destination trees from same-identity mutation.

## Resource contract

The encoded source, decoded pixels/allocation, crop bounds, and output buffer
are checked against `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB),
`IRONFLOW_MAX_IMAGE_PIXELS` (25 million), and
`IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB). The working estimate
includes the retained source plus crop output. Decode, crop, and encode
run on a tracked blocking worker with cancellation checkpoints between opaque
operations.
