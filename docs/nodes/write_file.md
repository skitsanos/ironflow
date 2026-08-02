# `write_file`

Atomically write text, streamed Base64, or a verified workflow artifact to a
regular file. The work runs on a tracked blocking worker and is bounded by
`IRONFLOW_MAX_FILE_BYTES` (default 50 MiB).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Destination path. Supports `${ctx.key}` interpolation. Missing parent directories are created. |
| `content` | string | no | `""` | Inline text or Base64 input. Supports interpolation. Mutually exclusive with `source_key` and `artifact`. |
| `source_key` | string | no | — | Context key containing a string or artifact descriptor. Mutually exclusive with `content` and `artifact`. |
| `artifact` | object or string | no | — | Artifact descriptor or canonical `artifact://sha256/...` URI. Mutually exclusive with `content` and `source_key`. |
| `encoding` | string | no | `"text"` | `"text"`, `"base64"`, or `"artifact"`. An object source is recognized as an artifact without the explicit encoding. |
| `append` | bool | no | `false` | Copy the existing regular file followed by the new input into an atomic replacement. |

At most one input form may be configured. Omitting all three writes an empty
file. A canonical artifact URI stored as a context string requires
`encoding = "artifact"`; artifact descriptor objects are detected directly.

## Safety and resource contract

- Base64 decoded length is admitted before the worker or decoded-byte
  allocation, then decoded in bounded chunks directly into the staged file.
- Artifact content is copied from the same verified, rewound handle whose size
  and SHA-256 identity were checked by the artifact store.
- Append mode applies the byte limit to the resulting file, not only the new
  input. The existing destination must be a regular non-link file.
- Overwrite and append both use a sibling staged file. Malformed input, size
  failure, cancellation, flush/sync failure, or commit failure leaves an
  existing destination unchanged and removes the staged file.
- Final links are refused. Unix uses handle-relative staging and replacement.
  Portable platforms revalidate the destination immediately before an OS-level
  atomic replacement, but cannot close a hostile parent-directory swap race;
  protect destination trees from same-identity mutation.

Artifact descriptors remain a local-store capability. All hosts that may run a
workflow need the same protected `IRONFLOW_ARTIFACT_DIR`, and processes sharing
that directory must not share an untrusted OS identity.

## Context output

- `write_file_path` — resolved destination path.
- `write_file_success` — `true` after the staged file is committed.

## Examples

```lua
local flow = Flow.new("write_demo")

flow:step("write", nodes.write_file({
    path = "/tmp/ironflow.txt",
    content = "Hello from IronFlow"
}))

flow:step("append", nodes.write_file({
    path = "/tmp/ironflow.txt",
    content = "\nAppended safely.",
    append = true
})):depends_on("write")

-- A previous node placed an artifact descriptor in ctx.cached_artifact.
flow:step("restore", nodes.write_file({
    path = "/tmp/restored.bin",
    source_key = "cached_artifact"
}))

return flow
```
