-- basic_subworkflow.lua — Call a subworkflow and use its output

local flow = Flow.new("basic_subworkflow")

flow:step("set_name", nodes.code({
    source = function(ctx)
        return { name = "IronFlow" }
    end
}))

flow:step("call_greet", nodes.subworkflow({
    flow = "greet.lua",
    input = {
        name = "name"   -- map parent "name" → child "name"
    }
})):depends_on("set_name")

flow:step("show_result", nodes.log({
    message = "Subworkflow returned: ${ctx.greeting}",
    level = "info"
})):depends_on("call_greet")

return flow

-- Run with:
--   ironflow run examples/11-subworkflow/basic_subworkflow.lua
