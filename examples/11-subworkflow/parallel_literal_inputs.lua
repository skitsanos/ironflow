local flow = Flow.new("parallel_literal_inputs")

flow:step("prepare", function()
    return {
        customer = { id = "C-42" },
        jobs = { "customer", false, json_null, { id = "customer" }, json_array({}), json_object({}) }
    }
end)

flow:step("dynamic", nodes.parallel_subworkflows({
    flow = "literal_item_child.lua",
    source_key = "jobs",
    item_key = "job",
    index_key = "ordinal",
    input = { customer_copy = "customer" },
    child_output_key = "child",
    output_key = "literal_results",
    max_concurrent = 2
})):depends_on("prepare")

flow:step("static", nodes.parallel_subworkflows({
    flows = {
        { flow = "literal_item_child.lua", input = { job = "customer", ordinal = 1 }, output_key = "child" }
    },
    output_key = "mapped_results"
})):depends_on("prepare")

flow:step("verify", function(ctx)
    for index, item in ipairs(ctx.jobs) do
        local child = ctx.literal_results[index].child
        assert(json_stringify(child.received) == json_stringify(item))
        assert(child.position == index)
        assert(child.customer_copy.id == ctx.customer.id)
    end
    -- The same string is literal in a source array, but a reference in input.
    assert(ctx.literal_results[1].child.received == "customer")
    assert(ctx.mapped_results[1].child.received.id == "C-42")
    return { literal_inputs_verified = true }
end):depends_on("dynamic", "static")

return flow
