# `validate_schema`

Validate a context value against a JSON Schema. The value is validated as stored; a string is not parsed.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Top-level context key whose value is validated |
| `schema` | object/boolean | One of | — | Inline JSON Schema |
| `schema_key` | string | One of | — | Context key holding a decoded schema or a JSON Schema string |

Give `schema` or `schema_key`; when both are set, `schema` is used.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `validation_success` | boolean | `true` when the value matches the schema |
| `validation_errors` | array | Error strings of the form `<message> at <instance path>`; empty on success |

## Behavior

- A string value is validated as a JSON string. To parse JSON text before validating it, use [`json_validate`](json_validate.md).
- Failed validation fails the step with `Schema validation failed: ...` and the errors joined by `; `. Downstream steps are skipped unless an `on_error` handler resolves the failure; the handler receives both outputs in `_error_output`.
- The step also fails when `source_key` or `schema_key` is missing, or the schema is invalid.
- Validation is offline: references bundled in the schema resolve, references that need network or file retrieval fail without fetching. See [JSON Schema validation](../JSON_SCHEMA_VALIDATION.md).

## Examples

### Inline schema

```lua
local flow = Flow.new("user_registration")

flow:step("validate", nodes.validate_schema({
    source_key = "payload",
    schema = {
        type = "object",
        required = { "email", "name" },
        properties = {
            email = { type = "string" },
            name  = { type = "string" },
            age   = { type = "integer", minimum = 0 }
        }
    }
}))

flow:step("done", nodes.log({
    message = "Validation passed for ${ctx.payload.name} (${ctx.payload.email})"
})):depends_on("validate")

return flow
```

### Validate with a schema loaded from context

```lua
local flow = Flow.new("validate_from_context")

flow:step("read_schema", nodes.read_file({
    path = "schemas/user.schema.json",
    output_key = "schema"
}))

flow:step("validate", nodes.validate_schema({
    source_key = "payload",
    schema_key = "schema_content"
})):depends_on("read_schema")

return flow
```
