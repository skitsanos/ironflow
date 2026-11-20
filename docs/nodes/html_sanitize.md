# `html_sanitize`

Remove dangerous tags, attributes and scripts from HTML using the [ammonia](https://crates.io/crates/ammonia) allow-list sanitizer.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | One of `input` or `source_key` | — | Literal HTML; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `input` or `source_key` | — | Context key holding the HTML; a non-string value is serialized as JSON first. |
| `output_key` | string | No | `"sanitized_html"` | Context key for the sanitized HTML. |
| `allowed_tags` | string[] | No | ammonia defaults | Tags to keep, replacing the default allow-list; `{}` keeps none. |
| `strip_comments` | boolean | No | `true` | Remove HTML comments. |
| `link_rel` | string | No | `"noopener noreferrer"` | `rel` attribute set on links; supports `${ctx.key}` interpolation. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Sanitized HTML. |

## Behavior

- Setting both `input` and `source_key`, or neither, fails the step; so does a `source_key` that is not in the context.
- With `allowed_tags` set, only the listed tags are kept; other tags are removed and their text stays. An empty list (`{}`) leaves text only.
- `script` and `style` are always removed with their content; listing either in `allowed_tags` fails the step.
- An `allowed_tags` value that is not a list of strings, or a `link_rel` that is not a string, fails the step instead of falling back to the default.

## Examples

### Basic sanitization

```lua
flow:step("sanitize", nodes.html_sanitize({
    input = '<h1>Hello</h1><script>alert("xss")</script>',
}))
-- Output: { sanitized_html = "<h1>Hello</h1>" }
```

### Custom allowed tags

```lua
flow:step("sanitize", nodes.html_sanitize({
    input = '<p><b>bold</b> <custom>tag</custom></p>',
    allowed_tags = { "p", "b", "custom" },
}))
```

### Text only

```lua
flow:step("sanitize", nodes.html_sanitize({
    input = '<p>Plain <b>text</b> only</p>',
    allowed_tags = {},
}))
-- Output: { sanitized_html = "Plain text only" }
```
