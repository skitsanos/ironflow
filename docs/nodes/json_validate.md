# `json_validate`

Validate JSON from context against a JSON Schema.

## Parameters

| Parameter    | Type   | Required | Default | Description                                                                    |
|--------------|--------|----------|---------|--------------------------------------------------------------------------------|
| `source_key` | string | Yes      | --      | Context key containing either a JSON string, or a value already decoded as JSON |
| `schema`     | object | Yes      | --      | A JSON Schema object to validate the data against                              |

If the value is a string, `json_validate` first parses it as JSON, then validates the result.  
If the value is already a JSON object/array/etc., it is validated directly.

## Context Output

- `validation_success` — boolean indicating whether validation passed
- `validation_errors` — array of validation error strings (empty on success)

The node returns an error when validation fails so downstream steps are skipped unless
`on_error` is configured.

## Example

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
