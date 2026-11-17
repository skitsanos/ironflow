# `hash`

Compute a SHA-256, SHA-384, SHA-512 or MD5 hash of a string or context value.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `algorithm` | string | No | `"sha256"` | `sha256`, `sha-256`, `sha384`, `sha-384`, `sha512`, `sha-512` or `md5` (any case) |
| `input` | string | One of | — | Literal string to hash; supports `${ctx.key}` interpolation |
| `source_key` | string | One of | — | Context key whose value is hashed |
| `output_key` | string | No | `"hash"` | Context key for the hex digest |

Give `input` or `source_key`; when both are set, `input` is used.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Lowercase hex digest |
| `{output_key}_algorithm` | string | The `algorithm` value as written in the config |

## Behavior

- A string value under `source_key` is hashed as is; any other value is serialized to compact JSON first.
- An unsupported algorithm, or a missing `source_key`, fails the step.

## Examples

### Hash an interpolated value

```lua
local flow = Flow.new("hash_email")

flow:step("hash_it", nodes.hash({
    input = "${ctx.user.email}",
    algorithm = "sha256",
    output_key = "email_hash"
}))

flow:step("done", nodes.log({
    message = "Email hash: ${ctx.email_hash}"
})):depends_on("hash_it")

return flow
```

### Hash a context value with the default output key

```lua
local flow = Flow.new("hash_password")

flow:step("hash_pw", nodes.hash({
    source_key = "password",
    algorithm = "sha512"
}))

flow:step("done", nodes.log({
    message = "Password SHA-512: ${ctx.hash}"
})):depends_on("hash_pw")

return flow
```
