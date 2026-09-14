-- Offline IF-135 example. Keep if135_error_child.lua beside this file.
-- Run with a build containing IF-135:
--   ironflow run examples/11-subworkflow/if135_child_error_details.lua
-- Expected: parent success, child_error_details_verified = true, and errors
-- containing both "task 'parse'" and "boom: unsupported file type".

local flow = Flow.new("if135_child_error_details")

flow:step("namespaced", nodes.subworkflow({
    flow = "if135_error_child.lua",
    input = { should_fail = true },
    output_key = "document",
    on_error = "ignore"
}))

flow:step("check_namespaced", function(ctx)
    assert(ctx.document_success == false, "child failure was hidden")
    assert(ctx.document.child_prepared == true, "partial context was lost")
    assert(type(ctx.document_error) == "string", "child error detail is missing")
    assert(ctx.document_error == ctx.subworkflow_error, "error aliases disagree")
    assert(string.find(ctx.document_error, "task 'parse'", 1, true))
    assert(string.find(ctx.document_error, "boom: unsupported file type", 1, true))
    return { namespaced_error_detail = ctx.document_error }
end):depends_on("namespaced")

flow:step("unnamespaced", nodes.subworkflow({
    flow = "if135_error_child.lua",
    input = { should_fail = true },
    on_error = "ignore"
})):depends_on("check_namespaced")

flow:step("check_unnamespaced", function(ctx)
    assert(ctx.subworkflow_success == false, "child failure was hidden")
    assert(ctx.child_prepared == true, "partial context was lost")
    assert(type(ctx.subworkflow_error) == "string", "child error detail is missing")
    assert(string.find(ctx.subworkflow_error, "task 'parse'", 1, true))
    assert(string.find(ctx.subworkflow_error, "boom: unsupported file type", 1, true))
    return { unnamespaced_error_detail = ctx.subworkflow_error }
end):depends_on("unnamespaced")

flow:step("parallel", nodes.parallel_subworkflows({
    flows = {
        { flow = "if135_error_child.lua", input = { should_fail = true } },
        { flow = "if135_error_child.lua", input = { should_fail = false } }
    },
    output_key = "documents",
    on_error = "ignore",
    max_concurrent = 2
})):depends_on("check_unnamespaced")

flow:step("verify", function(ctx)
    local failed = ctx.documents[1]
    local succeeded = ctx.documents[2]
    assert(ctx.documents_count == 2 and ctx.documents_errors == 1)
    assert(ctx.documents_all_succeeded == false)
    assert(failed.success == false and failed.child_prepared == true)
    assert(type(failed.error) == "string", "parallel child error detail is missing")
    assert(string.find(failed.error, "task 'parse'", 1, true))
    assert(string.find(failed.error, "boom: unsupported file type", 1, true))
    assert(succeeded.success == true and succeeded.parsed == true)
    assert(succeeded.error == nil, "successful child has failure metadata")
    return {
        parallel_error_detail = failed.error,
        child_error_details_verified = true
    }
end):depends_on("parallel")

return flow
