-- Validates a JSON string payload using an offline bundled schema definition.
local flow = Flow.new("json_validate_flow")

flow:step("prepare_input", nodes.code({
    source = function(ctx)
        local payload_json = ctx.payload_json
        if payload_json == nil then
            payload_json = '{"id":"ORD-123","name":"Alice","status":"new","age":29}'
        end
        return { payload_json = payload_json }
    end,
}))

flow:step("validate", nodes.json_validate({
    source_key = "payload_json",
    schema = {
        ["$schema"] = "https://json-schema.org/draft/2020-12/schema",
        ["$defs"] = {
            status = { type = "string", enum = { "new", "processing", "done" } },
        },
        type = "object",
        required = { "id", "name", "status" },
        properties = {
            id = { type = "string" },
            name = { type = "string" },
            status = { ["$ref"] = "#/$defs/status" },
            age = { type = "integer", minimum = 0 }
        }
    }
})):depends_on("prepare_input")

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
