-- Inspect provider error responses without failing the HTTP node.
-- `fail_on_status = false` keeps status, headers, body, success, and attempts
-- in context so the flow can route or classify the response itself.

local flow = Flow.new("status_inspection_retry")

flow:step("seed", nodes.code({
    source = function(ctx)
        return {
            target_status = ctx.target_status or "429"
        }
    end
}))

flow:step("probe", nodes.http_get({
    url = "https://httpbin.org/status/${ctx.target_status}",
    output_key = "provider",
    fail_on_status = false,
    retry_statuses = { 429, 500, 502, 503 },
    status_retries = 1,
    status_retry_backoff = 0,
    respect_retry_after = true,
    max_retry_after = 1
})):depends_on("seed")

flow:step("route", nodes.if_http_status({
    status_key = "provider_status",
    _step_name = "provider",
    default_route = "other",
    routes = {
        ["2xx"] = "ok",
        ["401"] = "auth_error",
        ["402"] = "billing_error",
        ["429"] = "rate_limited",
        ["5xx"] = "provider_error"
    }
})):depends_on("probe")

flow:step("ok", nodes.log({
    message = "Provider returned ${ctx.provider_status} after ${ctx.provider_attempts} attempt(s)",
    level = "info"
})):depends_on("route"):route("ok")

flow:step("auth_error", nodes.log({
    message = "Authentication failed with status ${ctx.provider_status}",
    level = "warn"
})):depends_on("route"):route("auth_error")

flow:step("billing_error", nodes.log({
    message = "Billing or quota issue with status ${ctx.provider_status}",
    level = "warn"
})):depends_on("route"):route("billing_error")

flow:step("rate_limited", nodes.log({
    message = "Rate limited after ${ctx.provider_attempts} attempt(s)",
    level = "warn"
})):depends_on("route"):route("rate_limited")

flow:step("provider_error", nodes.log({
    message = "Provider server error: ${ctx.provider_status}",
    level = "error"
})):depends_on("route"):route("provider_error")

flow:step("other", nodes.log({
    message = "Unhandled status ${ctx.provider_status}, success=${ctx.provider_success}",
    level = "warn"
})):depends_on("route"):route("other")

return flow
