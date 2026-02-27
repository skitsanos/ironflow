-- Demonstrates http_request, http_put, and http_delete nodes

local flow = Flow.new("http_methods")

-- Generic http_request with explicit method
flow:step("generic_get", nodes.http_request({
    method = "GET",
    url = "https://httpbin.org/get",
    output_key = "generic"
}))

-- PUT request
flow:step("put_data", nodes.http_put({
    url = "https://httpbin.org/put",
    body = { name = "Alice", role = "admin" },
    output_key = "put_result"
}))

-- DELETE request
flow:step("delete_item", nodes.http_delete({
    url = "https://httpbin.org/delete",
    output_key = "delete_result"
}))

flow:step("log_results", nodes.log({
    message = "Generic: ${ctx.generic_status}, PUT: ${ctx.put_result_status}, DELETE: ${ctx.delete_result_status}"
})):depends_on("generic_get", "put_data", "delete_item")

return flow
