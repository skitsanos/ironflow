-- input_mapping_forms.lua — The three forms of a child `input` mapping.
--
--   child_key = "parent_key"                 the parent key must exist
--   child_key = { optional = "parent_key" }  omitted from the child when absent
--   child_key = { literal = <value> }        passed as written, never looked up
--
-- A plain string that names no parent key fails the step before the child
-- starts; it is not passed as text and the entry is not dropped.
-- Run: ironflow run examples/11-subworkflow/input_mapping_forms.lua

local flow = Flow.new("input_mapping_forms")

flow:step("prepare", function()
    return { visitor = "Ada" }
end)

flow:step("required", nodes.subworkflow({
    flow = "greet.lua",
    input = { name = "visitor" },
    output_key = "required_result"
})):depends_on("prepare")

-- `nickname` is not in the parent context, so the child runs without `name`.
flow:step("optional", nodes.subworkflow({
    flow = "greet.lua",
    input = { name = { optional = "nickname" } },
    output_key = "optional_result"
})):depends_on("prepare")

-- Without `literal`, "visitor" would select the parent key.
flow:step("literal", nodes.subworkflow({
    flow = "greet.lua",
    input = { name = { literal = "visitor" } },
    output_key = "literal_result"
})):depends_on("prepare")

-- A missing required key is a step error; the child flow is never started.
flow:step("missing", nodes.subworkflow({
    flow = "greet.lua",
    input = { name = "nickname" },
    output_key = "missing_result"
})):depends_on("prepare"):on_error("report_missing")

flow:step("report_missing", function(ctx)
    return { missing_error = ctx._error_message }
end)

flow:step("verify", function(ctx)
    assert(ctx.required_result.greeting == "Hello, Ada!")
    assert(ctx.optional_result.greeting == "Hello, World!")
    assert(ctx.literal_result.greeting == "Hello, visitor!")
    assert(ctx.missing_result == nil, "the child must not have run")
    assert(ctx.missing_error:find("input 'name'", 1, true), ctx.missing_error)
    return { mapping_forms_verified = true }
end):depends_on("required", "optional", "literal", "report_missing")

return flow
