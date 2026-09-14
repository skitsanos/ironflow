local flow = Flow.new("parallel_result_metadata")

flow:step("prepare", function()
    return { jobs = { { should_fail = false }, { should_fail = true } } }
end)

flow:step("static", nodes.parallel_subworkflows({
    flows = {
        { flow = "metadata_child.lua", input = { should_fail = false } },
        { flow = "metadata_child.lua", input = { should_fail = true }, output_key = "child" }
    },
    output_key = "static_results",
    on_error = "ignore",
    max_concurrent = 2
}))

flow:step("dynamic", nodes.parallel_subworkflows({
    flow = "metadata_child.lua",
    source_key = "jobs",
    child_output_key = "child",
    output_key = "dynamic_results",
    on_error = "ignore",
    max_concurrent = 2
})):depends_on("prepare")

flow:step("verify", function(ctx)
    local good = ctx.static_results[1]
    assert(good.success == true and good.flow == "metadata_child")
    assert(good.error == nil and good.value == 42)

    -- Domain fields remain available inside a non-reserved child namespace.
    local failed = ctx.static_results[2]
    assert(failed.success == false and failed.flow == "metadata_child")
    assert(type(failed.error) == "string")
    assert(failed.child.success == true and failed.child.flow == "domain-flow")
    assert(failed.child.error == "domain-note")

    for index, result in ipairs(ctx.dynamic_results) do
        assert(result.flow == "metadata_child")
        assert(result.success == (index == 1))
        assert(result.child.success == (index == 2))
    end
    assert(ctx.static_results_errors == 1 and ctx.dynamic_results_errors == 1)
    assert(ctx.static_results_all_succeeded == false)
    assert(ctx.dynamic_results_all_succeeded == false)
    return { metadata_verified = true }
end):depends_on("static", "dynamic")

return flow
