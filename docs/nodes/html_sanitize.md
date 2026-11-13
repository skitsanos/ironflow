# `html_sanitize`

Sanitize HTML by removing dangerous tags, attributes, and scripts using the [ammonia](https://crates.io/crates/ammonia) crate.

## Parameters

| Parameter       | Type     | Required | Default              | Description                        |
|-----------------|----------|----------|----------------------|------------------------------------|
| `input`         | string   | *        |                      | HTML string (supports `${ctx.key}`)  |
| `source_key`    | string   | *        |                      | Context key containing HTML string |
| `output_key`    | string   | no       | `"sanitized_html"`   | Key to store sanitized output      |
| `allowed_tags`  | string[] | no       | ammonia defaults      | Tags to keep, replacing the default allow-list; `{}` keeps none. See [Allowed tags](#allowed-tags) |
| `strip_comments`| bool     | no       | `true`               | Whether to strip HTML comments     |
| `link_rel`      | string   | no       | `"noopener noreferrer"` | `rel` attribute for links       |

*One of `input` or `source_key` is required (not both).

## Allowed tags

- Omitted: ammonia's default allow-list applies.
- A list of tag names: only those tags are kept. Other tags are removed and
  their text content stays. Names are matched in lowercase.
- An empty list (`allowed_tags = {}`): no tags are kept, leaving text only.
- `script` and `style` cannot be allowed. These elements are always removed
  together with their content, and listing either one fails the step.
- Any other value (a string, a number, a keyed table, or a list with a
  non-string entry) fails the step. It is not replaced by the default
  allow-list, which could keep more tags than intended.

A failed step is an ordinary node error: retries and `on_error` apply.

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

### Text only

```lua
flow:step("sanitize", nodes.html_sanitize({
    input = '<p>Plain <b>text</b> only</p>',
    allowed_tags = {},
}))
-- Output: { sanitized_html = "Plain text only" }
```

### From context

```lua
flow:step("sanitize", nodes.html_sanitize({
    source_key = "raw_html",
    output_key = "clean_html",
}))
```
