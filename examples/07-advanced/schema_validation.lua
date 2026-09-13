-- Demonstrates offline JSON Schema validation with a bundled customer schema.
local flow = Flow.new("schema_validation")

flow:step("prepare_input", nodes.code({
    source = function(ctx)
        local order = ctx.order
        if order == nil then
            order = {
                id = "ORD-001",
                amount = 99.99,
                customer = {
                    name = "Alice",
                    email = "alice@example.com"
                }
            }
        end
        return { order = order }
    end,
}))

-- Validate the order object against a JSON schema
flow:step("validate", nodes.validate_schema({
    source_key = "order",
    schema = {
        ["$schema"] = "https://json-schema.org/draft/2020-12/schema",
        ["$defs"] = {
            customer = {
                type = "object",
                required = { "name", "email" },
                properties = {
                    name = { type = "string" },
                    email = { type = "string" }
                }
            },
        },
        type = "object",
        required = { "id", "amount", "customer" },
        properties = {
            id = { type = "string" },
            amount = { type = "number", minimum = 0 },
            customer = { ["$ref"] = "#/$defs/customer" },
        },
    }
})):depends_on("prepare_input")

-- Only runs if validation passes
flow:step("process", nodes.log({
    message = "Order ${ctx.order.id} validated — processing $${ctx.order.amount} for ${ctx.order.customer.name}",
    level = "info"
})):depends_on("validate")

return flow

-- Run with valid data:
--   ironflow run examples/07-advanced/schema_validation.lua \
--     --context '{"order":{"id":"ORD-001","amount":99.99,"customer":{"name":"Alice","email":"alice@example.com"}}}'
--
-- Run with invalid data (missing amount):
--   ironflow run examples/07-advanced/schema_validation.lua \
--     --context '{"order":{"id":"ORD-002","customer":{"name":"Bob","email":"bob@example.com"}}}'
