# `image_grayscale`

Convert a single image to grayscale.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path, artifact URI/descriptor, or explicit Base64 object |
| `output_path` | string | yes | — | Destination image path |
| `format` | string | no | inferred / `png` | `png` or `jpeg`/`jpg` |
| `output_key` | string | no | `"grayscale_image"` | Prefix for output values |

Artifact inputs are opened and SHA-256 verified inside the tracked blocking
worker; decoding consumes that same rewound handle rather than a resolved store
pathname.

Supported source formats are BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM,
QOI, TGA, TIFF, and WebP. Output remains restricted to PNG or JPEG.

## Context Output

- `<output_key>` — output file path
- `<output_key>_width` / `<output_key>_height`
- `<output_key>_format`
- `<output_key>_success`

## Example

```lua
local flow = Flow.new("image_grayscale_demo")

flow:step("grayscale", nodes.image_grayscale({
    path = "examples/fixtures/ironflow-sample.png",
    output_path = "output/sample_front_gray.png",
    output_key = "gray"
}))

flow:step("log", nodes.log({
    message = "Grayscale image: ${ctx.gray_width}x${ctx.gray_height}"
})):depends_on("grayscale")

return flow
```

## Destination contract

`output_path` is published through the same rooted, atomic writer as
[`write_file`](write_file.md), inside the tracked blocking worker: the
grayscale image is encoded into a staged file beside the destination, flushed
and synchronized, then renamed over it.

- The destination directory must already exist. The node creates no
  directories and fails with `image_grayscale: destination directory '<dir>'
  does not exist`. Directory aliases, including macOS `/tmp`, are resolved
  first.
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

Image headers and decoded byte requirements are checked before allocation using
`IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB), `IRONFLOW_MAX_IMAGE_PIXELS` (25
million), and `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB). The
working estimate includes retained source plus grayscale output. Decode,
grayscale conversion, and encode run on a tracked blocking worker with
cancellation checkpoints between opaque operations.
