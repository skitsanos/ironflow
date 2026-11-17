# `slack_notification`

Post a message to Slack through an incoming webhook.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `webhook_url` | string | No | `SLACK_WEBHOOK` | Incoming webhook URL; interpolated. Required when the variable is unset. |
| `text` | string | No | — | Message text; interpolated. Replaces `payload.text`. |
| `message` | string | No | — | Alias for `text`, used when `text` is absent. |
| `payload` | object | No | `{}` | Slack payload; every string value is interpolated. |
| `timeout` | number | No | `30` | Seconds; must be positive. |
| `output_key` | string | No | `"slack"` | Prefix for output keys. |

One of `text`, `message` or a string `payload.text` is required.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_status` | number | HTTP status |
| `{output_key}_data` | any | Response body, parsed as JSON if possible, else text |
| `{output_key}_success` | boolean | `true` |

## Behavior

- A non-2xx response fails the step with the status and redacted body.
- A failed response read does not prove the message was not delivered; retrying can post it twice.
- Redirects are followed only within the same origin, up to 10.
- The operator's [outbound destination policy](../SECURITY.md#outbound-destinations) applies to the webhook URL.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_HTTP_BODY_BYTES` | `52428800` | Maximum webhook response body |

## Examples

### Post with a custom payload

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
