-- Route HTTP flows based on status code.
-- 1) Choose a status to probe via context (`target_status`).
-- 2) Send request to that status endpoint.
-- 3) Route by exact code or class using `if_http_status`.

local flow = Flow.new("if_http_status")

flow:step("seed", nodes.code({
    source = function(ctx)
        return {
            target_status = ctx.target_status or "200"
        }
    end
}))

flow:step("probe", nodes.http_get({
    url = "https://httpbin.org/status/${ctx.target_status}",
    output_key = "probe"
})):depends_on("seed")

flow:step("route", nodes.if_http_status({
    status_key = "probe_status",
    _step_name = "probe",
    default_route = "unexpected",
    routes = {
        ["2xx"] = "success",
        ["401"] = "unauthorized",
        ["429"] = "rate_limited"
    }
})):depends_on("probe")

flow:step("success", nodes.log({
    message = "HTTP ${ctx._status_code_probe} success",
    level = "info"
})):depends_on("route"):route("success")

flow:step("unauthorized", nodes.log({
    message = "HTTP ${ctx._status_code_probe} unauthorized",
    level = "warn"
})):depends_on("route"):route("unauthorized")

flow:step("rate_limited", nodes.log({
    message = "HTTP ${ctx._status_code_probe} rate limited",
    level = "warn"
})):depends_on("route"):route("rate_limited")

flow:step("unexpected", nodes.log({
    message = "HTTP ${ctx._status_code_probe} class ${ctx._status_class_probe} encountered",
    level = "error"
})):depends_on("route"):route("unexpected")

return flow
