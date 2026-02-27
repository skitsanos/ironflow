# `slack_notification`

Post messages to Slack using an incoming webhook URL.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `webhook_url` | string | no | env `SLACK_WEBHOOK` | Slack Incoming Webhook URL. |
| `text` | string | no | -- | Plain text message body. |
| `message` | string | no | -- | Alias for `text`. |
| `payload` | object | no | `{}` | Full Slack payload object to send. |
| `timeout` | number | no | `30` | Request timeout in seconds. |
| `output_key` | string | no | `"slack"` | Prefix for context output keys. |

Either `text`/`message` or `payload.text` is required.

`webhook_url` is optional and may be omitted if `SLACK_WEBHOOK` is set in env.

If `payload` is provided, all string values are context-interpolated (`${ctx.key}`).

## Context Output

- `{output_key}_status` — HTTP status code.
- `{output_key}_data` — Response body (JSON parsed if possible, otherwise raw text).
- `{output_key}_success` — `true` on HTTP 2xx success.

## Example

```lua
local flow = Flow.new("slack_notification")

flow:step("notify", nodes.slack_notification({
    webhook_url = env("SLACK_WEBHOOK"),
    text = "Workflow completed",
    payload = {
        username = "IronFlow",
        channel = "#alerts"
    }
})):depends_on("run_job")

flow:step("log", nodes.log({
    message = "Slack status: ${ctx.slack_status}"
})):depends_on("notify")

return flow
```
