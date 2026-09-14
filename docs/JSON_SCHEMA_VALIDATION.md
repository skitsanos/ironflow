# JSON Schema Validation

`validate_schema` and `json_validate` compile schemas in **offline mode**.
The same policy applies to inline `schema` values and schemas loaded through
`schema_key`, whether decoded JSON or JSON text.

## References

- References inside the supplied document are supported, including `$defs`,
  JSON Pointer fragments, anchors, and embedded resources identified by `$id`.
- Standard draft meta-schemas supported by the bundled library are available
  without network access. An HTTP(S) `$schema` identifier for a standard draft
  does not cause a fetch.
- References that require retrieval of another resource are rejected, including
  HTTP(S), `file:`, unresolved relative references, and custom meta-schemas that
  are not bundled. There is no configuration switch to enable retrieval.
- Relative or absolute references may still work when their target is an embedded
  resource already present in the supplied schema. Offline mode does not reject
  strings merely because they look like URLs.

Bundle shared definitions into the schema instead of relying on implicit I/O:

```lua
schema = {
    ["$schema"] = "https://json-schema.org/draft/2020-12/schema",
    ["$defs"] = {
        identifier = { type = "string", minLength = 1 },
    },
    type = "object",
    required = { "id" },
    properties = {
        id = { ["$ref"] = "#/$defs/identifier" },
    },
}
```

A `read_file` step can explicitly load the root schema for `schema_key`, but
this does not grant the validator filesystem access or make the file's directory
a reference search path. Bundle its dependencies too.

## Errors

Malformed schemas and unresolved references fail the node normally; unresolved
reference diagnostics omit the original URI to avoid disclosing credentials. These are
schema-compilation errors, not data-validation results, so they do not publish
`validation_success` or `validation_errors`. An `on_error` handler can still
recover the failed step.

Data that fails a successfully compiled schema retains the existing structured
failure output: `validation_success = false` and a populated `validation_errors`
array. Valid data returns `true` and an empty error array.

## Execution

Compilation and validation run on the blocking pool with tracked worker lifetime.
Cancellation and deadlines are checked before/after compilation, between reported
validation errors, and before returning results. Task/run admission remains held
until physical work exits; no late success is published after cancellation.

The library cannot be preempted inside an individual compilation or validation
call. Cancellation may therefore wait for that call to return. This policy removes
implicit network/file retrieval, but does not introduce a hard CPU, input-size,
error-count, or process-memory limit for arbitrary local schemas. Callers should
continue to use trusted, reasonably sized schemas and bounded workflow inputs.
