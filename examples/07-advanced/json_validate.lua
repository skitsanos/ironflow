-- Validates a JSON string payload before parsing
local flow = Flow.new("json_validate_flow")

flow:step("validate", nodes.json_validate({
    source_key = "payload_json",
    schema = {
        type = "object",
        required = { "id", "name", "status" },
        properties = {
            id = { type = "string" },
            name = { type = "string" },
            status = { type = "string", enum = { "new", "processing", "done" } },
            age = { type = "integer", minimum = 0 }
        }
    }
}))

flow:step("parse", nodes.json_parse({
    source_key = "payload_json",
    output_key = "payload"
})):depends_on("validate")

flow:step("log", nodes.log({
    message = "Validated payload ${ctx.payload.id} for ${ctx.payload.name} (status: ${ctx.payload.status})",
    level = "info"
})):depends_on("parse")

return flow

-- Run:
-- ironflow run examples/07-advanced/json_validate.lua \
--context '{"payload_json":"{\"id\":\"ORD-123\",\"name\":\"Alice\",\"status\":\"new\",\"age\":29}"}'
