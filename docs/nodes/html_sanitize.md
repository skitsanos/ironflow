# `html_sanitize`

Sanitize HTML by removing dangerous tags, attributes, and scripts using the [ammonia](https://crates.io/crates/ammonia) crate.

## Parameters

| Parameter       | Type     | Required | Default              | Description                        |
|-----------------|----------|----------|----------------------|------------------------------------|
| `input`         | string   | *        |                      | HTML string (supports `${ctx.*}`)  |
| `source_key`    | string   | *        |                      | Context key containing HTML string |
| `output_key`    | string   | no       | `"sanitized_html"`   | Key to store sanitized output      |
| `allowed_tags`  | string[] | no       | ammonia defaults      | Custom set of allowed HTML tags    |
| `strip_comments`| bool     | no       | `true`               | Whether to strip HTML comments     |
| `link_rel`      | string   | no       | `"noopener noreferrer"` | `rel` attribute for links       |

*One of `input` or `source_key` is required (not both).

## Context Output

| Key            | Type   | Description          |
|----------------|--------|----------------------|
| `{output_key}` | string | Sanitized HTML string |

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

### From context

```lua
flow:step("sanitize", nodes.html_sanitize({
    source_key = "raw_html",
    output_key = "clean_html",
}))
```
