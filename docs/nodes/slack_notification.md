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

## HTTP Transport

Webhook redirects follow only the original scheme, host, and effective port,
with at most 10 hops. Cross-origin redirects fail before replaying notification
content. Automatic `Referer` is disabled so the token-bearing webhook path is
not copied into that header. Configure a new webhook URL explicitly when its
origin changes.

Success and error response bodies share the HTTP limit
`IRONFLOW_MAX_HTTP_BODY_BYTES` (default `52428800`, 50 MiB). An oversized
`Content-Length` is rejected before reading the body; actual bytes are checked
before each chunk is retained, including responses without a declared length.
Exceeding the cap, a body-read failure, timeout, or run cancellation fails or
cancels the step without publishing partial notification output. The cap applies
to received body bytes before text decoding, not total process memory or decoded
JSON size. Charset/BOM decoding and JSON-or-text output behavior are unchanged.

Non-2xx responses within the cap still fail with their status and redacted body
detail. A size error takes precedence over provider body parsing when the cap is
exceeded. A failed response read does not prove the message was not delivered;
retrying a notification can send it again.

## Context Output

- `{output_key}_status` — HTTP status code.
- `{output_key}_data` — Response body (JSON parsed if possible, otherwise raw text).
- `{output_key}_success` — `true` on HTTP 2xx success.

## Example

The runnable [Slack example](../../examples/14-notifications/slack_notification.lua)
uses `SLACK_WEBHOOK`, which can point to a local HTTP fixture for offline testing.

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
