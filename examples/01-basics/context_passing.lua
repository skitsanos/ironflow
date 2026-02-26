-- Demonstrates how context flows between steps
local flow = Flow.new("context_passing")

-- Step 1: Template creates a value and stores it in context
flow:step("create_greeting", nodes.template_render({
    template = "Hello, ${ctx.first_name} ${ctx.last_name}!",
    output_key = "greeting"
}))

-- Step 2: Log reads from context set by step 1
flow:step("show", nodes.log({
    message = "Generated: ${ctx.greeting}",
    level = "info"
})):depends_on("create_greeting")

return flow

-- Run with:
--   ironflow run examples/01-basics/context_passing.lua \
--     --context '{"first_name":"John","last_name":"Doe"}'
