# `send_email`

Send an email through the Resend API or an SMTP relay.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `provider` | string | No | `"resend"` | `resend` or `smtp`. |
| `to` | string or array | Yes | — | Recipient address or addresses; interpolated. |
| `subject` | string | Yes | — | Subject line; interpolated. |
| `from` | string | No | `SENDER_EMAIL`, else `"onboarding@resend.dev"` | Sender address; interpolated. |
| `html` | string | No | — | HTML body; interpolated. |
| `text` | string | No | — | Plain-text body; interpolated. |
| `cc` | string or array | No | — | CC recipients. |
| `bcc` | string or array | No | — | BCC recipients. |
| `reply_to` | string or array | No | — | Reply-To address; SMTP takes a single string. |
| `timeout` | number | No | `30` | Seconds; must be positive. |
| `output_key` | string | No | `"email"` | Prefix for output keys. |

### Resend

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `api_key` | string | Yes | `RESEND_API_KEY` | Resend API key. |
| `api_url` | string | No | `"https://api.resend.com/emails"` | Resend-compatible endpoint. |

### SMTP

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `smtp_server` | string | Yes | `SMTP_SERVER` | Relay host name. |
| `smtp_port` | number | No | `SMTP_PORT`, else 587, 465 or 25 by `smtp_tls` | Relay port. |
| `smtp_username` | string | No | `SMTP_USERNAME` | Login user; used only with a password. |
| `smtp_password` | string | No | `SMTP_PASSWORD` | Login password. |
| `smtp_tls` | string | No | `"starttls"` | `starttls`, `tls` (implicit TLS) or `none`; any other value means `starttls`. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_status` | number or string | Resend HTTP status (number) or SMTP reply code (string) |
| `{output_key}_data` | any | Resend response body (JSON if parseable, else text), or SMTP `{ code, message }` |
| `{output_key}_success` | boolean | `true` |

## Behavior

- An unknown `provider` fails the step.
- A non-2xx Resend response or a negative SMTP reply fails the step with the status and redacted detail.
- A failed read of the Resend response does not prove the email was not sent; retrying can send it twice.
- With SMTP, `html` and `text` together are sent as `multipart/alternative`.
- Resend follows redirects only within the same origin, up to 10.
- The operator's [outbound destination policy](../SECURITY.md#outbound-destinations) applies to `api_url` and `smtp_server`.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_HTTP_BODY_BYTES` | `52428800` | Maximum Resend response body; does not apply to SMTP |

## Examples

### Resend API

```lua
local flow = Flow.new("send_email_resend")

flow:step("send", nodes.send_email({
    to = env("SENDER_EMAIL"),
    subject = "Welcome to IronFlow!",
    html = "<h1>Welcome!</h1><p>Your workflow engine is ready.</p>",
    text = "Welcome! Your workflow engine is ready."
}))

flow:step("log", nodes.log({
    message = "Email result: success=${ctx.email_success}, data=${ctx.email_data}"
})):depends_on("send")

return flow
```

### SMTP

```lua
flow:step("send", nodes.send_email({
    provider = "smtp",
    to = env("SENDER_EMAIL"),
    subject = "IronFlow SMTP Test",
    html = "<h1>Hello!</h1><p>Sent via SMTP.</p>",
    text = "Hello! Sent via SMTP."
}))
```

### Multiple recipients

```lua
flow:step("notify_team", nodes.send_email({
    to = { "alice@example.com", "bob@example.com" },
    from = "noreply@example.com",
    subject = "Deploy complete: ${ctx.version}",
    html = "<p>Version <b>${ctx.version}</b> deployed.</p>",
    cc = "manager@example.com",
    output_key = "deploy_email"
}))
```
