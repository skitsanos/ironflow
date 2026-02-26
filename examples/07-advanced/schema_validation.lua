-- Demonstrates JSON Schema validation
local flow = Flow.new("schema_validation")

-- Validate the order object against a JSON schema
flow:step("validate", nodes.validate_schema({
    source_key = "order",
    schema = {
        type = "object",
        required = { "id", "amount", "customer" },
        properties = {
            id = { type = "string" },
            amount = { type = "number", minimum = 0 },
            customer = {
                type = "object",
                required = { "name", "email" },
                properties = {
                    name = { type = "string" },
                    email = { type = "string" }
                }
            }
        }
    }
}))

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
