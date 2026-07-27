-- Nightly revenue report: fetch in parallel, reduce inline, notify.
-- Retry and timeout policy is declared per step, so failure handling lives in
-- the flow rather than inside the code each step runs.
local flow = Flow.new("nightly_report")

-- Neither step depends on the other, so the engine runs them in parallel.
-- retries(3, 1) allows 4 attempts with 1s -> 2s -> 4s backoff, and timeout(30)
-- is a deadline for the whole step, retries included.
flow:step("users", nodes.http_get({
    url = "https://api.example.com/users",
    output_key = "users"
})):retries(3, 1):timeout(30)

flow:step("orders", nodes.http_get({
    url = "https://api.example.com/orders",
    output_key = "orders"
})):retries(3, 1):timeout(30)

-- Waits for both, then reduces them inline. Whatever the handler returns is
-- merged into the context under its own keys.
flow:step("totals", function(ctx)
    local orders = ctx.orders_data or {}
    local users = ctx.users_data or {}
    local revenue = 0

    for _, order in ipairs(orders) do
        revenue = revenue + (order.amount or 0)
    end

    return {
        revenue = revenue,
        order_count = #orders,
        user_count = #users
    }
end):depends_on("users", "orders")

-- Runs once totals commits. Two further attempts if Slack is unreachable.
flow:step("notify", nodes.slack_notification({
    text = "Nightly report: $${ctx.revenue} across ${ctx.order_count} orders",
    payload = {
        channel = "#revenue",
        username = "IronFlow"
    },
    output_key = "slack"
})):depends_on("totals"):retries(2, 1)

return flow

-- The endpoints above are placeholders. Point them at your own API, set
-- SLACK_WEBHOOK, and run with:
--   ironflow run examples/00-showcase/nightly_report.lua --verbose
