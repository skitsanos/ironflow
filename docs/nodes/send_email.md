# `send_email`

Send an email via the Resend API or SMTP.

## Parameters

### Common Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `provider` | string | no | `"resend"` | Email provider: `"resend"` or `"smtp"`. |
| `to` | string or array | **yes** | -- | Recipient email address(es). Supports `${ctx.key}` interpolation. |
| `subject` | string | **yes** | -- | Email subject line. Supports interpolation. |
| `from` | string | no | env `SENDER_EMAIL` or `"onboarding@resend.dev"` | Sender email address. |
| `html` | string | no | -- | HTML body content. Supports interpolation. |
| `text` | string | no | -- | Plain text body content. Supports interpolation. |
| `cc` | string or array | no | -- | CC recipient(s). |
| `bcc` | string or array | no | -- | BCC recipient(s). |
| `reply_to` | string or array | no | -- | Reply-To address(es). |
| `timeout` | number | no | `30` | Request timeout in seconds. |
| `output_key` | string | no | `"email"` | Prefix for context output keys. |

### Resend-specific Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `api_key` | string | no | env `RESEND_API_KEY` | Resend API key. |
| `api_url` | string | no | `"https://api.resend.com/emails"` | Explicit Resend-compatible endpoint, including a local test fixture. |

### SMTP-specific Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `smtp_server` | string | no | env `SMTP_SERVER` | SMTP server hostname. |
| `smtp_port` | number | no | env `SMTP_PORT` or provider default | SMTP port (587 for STARTTLS, 465 for TLS, 25 for none). |
| `smtp_username` | string | no | env `SMTP_USERNAME` | SMTP authentication username. |
| `smtp_password` | string | no | env `SMTP_PASSWORD` | SMTP authentication password. |
| `smtp_tls` | string | no | `"starttls"` | TLS mode: `"starttls"` (default), `"tls"` (implicit), or `"none"`. |

## HTTP Transport

Resend API requests follow redirects only within the original scheme, host,
and effective port, with at most 10 hops. Cross-origin redirects fail before
replaying email content; automatic `Referer` is disabled. Configure the final
API endpoint explicitly when its origin changes. This policy does not change
the SMTP provider.

Resend success and error response bodies share `IRONFLOW_MAX_HTTP_BODY_BYTES`
(default `52428800`, 50 MiB). An oversized `Content-Length` fails before body
collection; actual bytes are checked before each chunk is retained, including
chunked responses. Exceeding the cap, an interrupted body, timeout, or run
cancellation fails or cancels the step without publishing partial email output.
The limit counts received body bytes before charset/BOM decoding. It is not a
total-memory or decoded-JSON limit. The SMTP provider is unaffected.

Within-limit non-2xx responses still fail with their status and redacted body
detail. Oversized responses fail with the size error before provider body parsing.
A failed response read does not prove the email was not delivered; retrying can
send the email again.

## Context Output

- `{output_key}_status` — HTTP status code (Resend) or SMTP response code.
- `{output_key}_data` — Response body (Resend JSON if parseable, otherwise decoded text) or SMTP response details.
- `{output_key}_success` — `true` on success.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RESEND_API_KEY` | Resend API key (fallback when `api_key` is not in config) |
| `SENDER_EMAIL` | Default sender address (fallback when `from` is not in config) |
| `SMTP_SERVER` | SMTP server hostname |
| `SMTP_PORT` | SMTP server port |
| `SMTP_USERNAME` | SMTP authentication username |
| `SMTP_PASSWORD` | SMTP authentication password |

## Examples

### Resend API

The runnable [Resend example](../../examples/14-notifications/send_email_resend.lua)
reads the optional `RESEND_API_URL` environment variable into `api_url` for a local
HTTP fixture; omitting it retains the normal API endpoint. This variable is an
example convention, not a node-level fallback. Keep API credentials synthetic
when testing against an alternate endpoint.

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
local flow = Flow.new("send_email_smtp")

flow:step("send", nodes.send_email({
    provider = "smtp",
    to = env("SENDER_EMAIL"),
    subject = "IronFlow SMTP Test",
    html = "<h1>Hello!</h1><p>Sent via SMTP.</p>",
    text = "Hello! Sent via SMTP."
}))

return flow
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
