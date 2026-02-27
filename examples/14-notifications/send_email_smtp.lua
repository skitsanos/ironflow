--[[
Send an email using SMTP.

Requires env vars: SMTP_SERVER, SMTP_USERNAME, SMTP_PASSWORD, SENDER_EMAIL
Optional: SMTP_PORT (default: 587 for STARTTLS)

TLS modes: "starttls" (default), "tls" (implicit TLS), "none" (no encryption)
]]

local flow = Flow.new("send_email_smtp")

-- Step 1: Send an email to yourself via SMTP
flow:step("send", nodes.send_email({
    provider = "smtp",
    to = env("SENDER_EMAIL"),
    subject = "IronFlow SMTP Test",
    html = "<h1>Hello from IronFlow!</h1><p>This email was sent via SMTP.</p>",
    text = "Hello from IronFlow! This email was sent via SMTP.",
    output_key = "email"
}))

-- Step 2: Log the result
flow:step("log", nodes.log({
    level = "info",
    message = "SMTP email sent: success=${ctx.email_success}, status=${ctx.email_status}"
})):depends_on("send")

return flow
