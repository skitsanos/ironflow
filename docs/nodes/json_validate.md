# `json_validate`

Validate JSON text or a JSON value from context against a JSON Schema. A string is parsed as JSON before validation.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding a JSON string or an already decoded value |
| `schema` | object/boolean | One of | — | Inline JSON Schema |
| `schema_key` | string | One of | — | Context key holding a decoded schema or a JSON Schema string |

Give `schema` or `schema_key`; when both are set, `schema` is used.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `validation_success` | boolean | `true` when the data matches the schema |
| `validation_errors` | array | Error strings of the form `<message> at <instance path>`; empty on success |

## Behavior

- A string value is parsed as JSON first; a syntax error fails the step. Any other value is validated as is. Use [`validate_schema`](validate_schema.md) to validate a string value itself.
- Failed validation fails the step with `Schema validation failed: ...` and the errors joined by `; `. Downstream steps are skipped unless an `on_error` handler resolves the failure; the handler receives both outputs in `_error_output`.
- The step also fails when `source_key` or `schema_key` is missing, or the schema is invalid.
- Validation is offline: references bundled in the schema resolve, references that need network or file retrieval fail without fetching. See [JSON Schema validation](../JSON_SCHEMA_VALIDATION.md).

## Examples

### Inline schema

```lua
local flow = Flow.new("json_validate_example")

flow:step("validate", nodes.json_validate({
    source_key = "payload_json",
    schema = {
        type = "object",
        required = { "id", "name" },
        properties = {
            id = { type = "string" },
            name = { type = "string" },
            age = { type = "integer", minimum = 0 }
        }
    }
}))

flow:step("ok", nodes.log({
    message = "Payload JSON passed validation."
})):depends_on("validate")

return flow
```

### Validate with a schema loaded from a file

```lua
local flow = Flow.new("json_validate_from_file")

flow:step("read_schema", nodes.read_file({
    path = "schemas/payload.schema.json",
    output_key = "schema"
}))

flow:step("validate", nodes.json_validate({
    source_key = "payload_json",
    schema_key = "schema_content"
})):depends_on("read_schema")

return flow
```
