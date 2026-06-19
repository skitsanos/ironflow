# `validate_schema`

Validate context data against a JSON Schema.

## Parameters

| Parameter    | Type   | Required | Default | Description                                              |
|--------------|--------|----------|---------|----------------------------------------------------------|
| `source_key` | string | Yes      | --      | Top-level context key whose value will be validated      |
| `schema`     | object | No*      | --      | A JSON Schema object to validate the data against        |
| `schema_key` | string | No*      | --      | Context key containing a JSON Schema object or JSON Schema string |

*Provide exactly one of `schema` or `schema_key`.

The node retrieves the value stored under `source_key` in the workflow context and validates it using the provided JSON Schema. If validation fails, the node returns an error and the workflow step is marked as failed.

Use this node when the value is already in your context as JSON (object/array/value).  
If the context value is a raw JSON string, use [`json_validate`](json_validate.md) instead.

## Context Output

- `validation_success` -- boolean indicating whether validation passed
- `validation_errors` -- array of error description strings (empty on success)

**Note:** When validation fails the node returns an error (`anyhow::bail!`), so downstream steps will not execute unless error handling is configured. The output keys are still populated before the error is raised.

## Example

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
