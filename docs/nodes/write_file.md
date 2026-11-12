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
- On Unix a new file gets `0666` masked by the process umask (`0644` under
  umask `022`). Overwrite and append keep the existing file's nine permission
  bits, so an appended `0755` script stays executable; setuid, setgid and
  sticky bits are not carried, and the replacement is owned by the IronFlow
  process. The staged file is mode `0600` until it is published, and missing
  parent directories are created `0755` masked by the umask. Other platforms
  use their native permission semantics.
- Writes to one destination are serialized inside the IronFlow process. Each
  write, overwrite or append, including `copy_file` and the `base64_decode`
  `output_file`, waits for the previous one to commit and observes
  cancellation and the step deadline while it waits. Parallel appends
  therefore keep every appended input; their order is the order in which the
  steps reached the file. The destination is identified by its resolved
  directory and case-folded file name.
- Writers outside that process are not coordinated. Two IronFlow processes
  that append to the same file, such as replicas sharing a volume, or another
  node that replaces it during an append, are last-writer-wins: an append can
  be discarded although its step succeeded. Give each process its own file.
- Final links are refused. Unix uses handle-relative staging and replacement.
  Portable platforms revalidate the destination immediately before an OS-level
  atomic replacement, but cannot close a hostile parent-directory swap race;
  protect destination trees from same-identity mutation.
- Configured parent-directory aliases, including macOS `/tmp`, are resolved
  before staging. Unix pins the resolved directory, so retargeting the alias
  afterward cannot redirect that write. This is not permission to follow a
  destination-file symlink or an archive entry's symlinked subdirectory.

Artifact descriptors are backend-neutral. Local mode requires the same
protected `IRONFLOW_ARTIFACT_DIR` on every possible consumer. S3 mode restores
missing bytes into each replica's private cache, verifies size and SHA-256, and
passes only the verified rewound handle to this node.

Remote restores observe step deadlines and run cancellation between received
chunks, not only when the response stalls. Cancellation discards the download's
private staging file before the tracked worker releases admission. The
destination stays unchanged, and no `write_file_success` is returned. Idle body
waits also poll cancellation every 100 ms; this is a cooperative check interval,
not a hard latency guarantee for filesystem operations. See the
[artifact lifecycle](../ARCHITECTURE.md#binary-artifacts) for the shared contract.

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
