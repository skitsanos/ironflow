# `hash`

Compute a cryptographic hash of a string or context value.

## Parameters

| Parameter    | Type   | Required | Default    | Description                                                            |
|--------------|--------|----------|------------|------------------------------------------------------------------------|
| `algorithm`  | string | No       | `"sha256"` | Hash algorithm to use (see Supported Algorithms)                       |
| `input`      | string | No*      | --         | Literal string to hash; supports `${ctx.*}` interpolation              |
| `source_key` | string | No*      | --         | Context key whose value will be hashed                                 |
| `output_key` | string | No       | `"hash"`   | Context key under which the hex-encoded hash is stored                 |

*Exactly one of `input` or `source_key` must be provided. If `input` is set, it is used (with context interpolation). Otherwise, the value is read from the context key specified by `source_key`. String context values are used directly; non-string values are JSON-serialized before hashing.

## Supported Algorithms

| Value                | Algorithm |
|----------------------|-----------|
| `sha256`, `sha-256`  | SHA-256   |
| `sha384`, `sha-384`  | SHA-384   |
| `sha512`, `sha-512`  | SHA-512   |
| `md5`                | MD5       |

Algorithm matching is case-insensitive.

## Context Output

- `{output_key}` -- hex-encoded hash string
- `{output_key}_algorithm` -- the algorithm name as provided in the config

## Example

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

Using `source_key` instead:

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

The output will be stored in the default key `hash` with the hex-encoded SHA-512 digest.
