--[[
Slack notification example using the `slack_notification` node.

This flow posts a JSON payload to a Slack webhook.
`webhook_url` can be provided explicitly as free text or left out to use
the `SLACK_WEBHOOK` environment variable.
]]

local flow = Flow.new("slack_notification")

--[[
Step 1: Send a message using webhook URL from environment variable.
If you want to hard-code the URL, uncomment `webhook_url` below.
]]
flow:step("notify", nodes.slack_notification({
    -- webhook_url = "https://hooks.slack.com/services/…",
    text = "IronFlow notification: notification example executed",
    payload = {
        channel = "#alerts",
        username = "IronFlow Bot",
        blocks = {
            {
                type = "section",
                text = {
                    type = "mrkdwn",
                    text = "IronFlow posted a *test* alert with structured payload."
                }
            }
        }
    },
    output_key = "slack"
}))

--[[
Step 2: Log what Slack returned.
]]
flow:step("log", nodes.log({
    level = "info",
    message = "Slack request done: status=${ctx.slack_status}, success=${ctx.slack_success}"
})):depends_on("notify")

return flow
