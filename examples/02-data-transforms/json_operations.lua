-- Demonstrates JSON parse, stringify, and field selection
local flow = Flow.new("json_operations")

flow:step("prepare_input", nodes.code({
    source = function()
        return {
            raw_json = [[
{"name":"Alice","email":"alice@example.com","age":30,"role":"admin"}
            ]]
        }
    end,
}))

-- Parse a JSON string from context into a structured value
flow:step("parse", nodes.json_parse({
    source_key = "raw_json",
    output_key = "parsed"
})):depends_on("prepare_input")

-- Select only specific fields from the parsed object
flow:step("pick_fields", nodes.select_fields({
    source_key = "parsed",
    fields = { "name", "email" },
    output_key = "user_summary"
})):depends_on("parse")

-- Serialize the selected fields back to a JSON string
flow:step("stringify", nodes.json_stringify({
    source_key = "user_summary",
    output_key = "result_json"
})):depends_on("pick_fields")

-- Log the result
flow:step("show", nodes.log({
    message = "Result: ${ctx.result_json}",
    level = "info"
})):depends_on("stringify")

return flow

-- Run with:
--   ironflow run examples/02-data-transforms/json_operations.lua \
--     --context '{"raw_json": "{\"name\":\"Alice\",\"email\":\"alice@example.com\",\"age\":30,\"role\":\"admin\"}"}'
