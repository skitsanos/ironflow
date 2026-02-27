--[[
Send an email using the Resend API.

Requires: RESEND_API_KEY env var (or pass api_key in config)
Optional: SENDER_EMAIL env var for default sender address
]]

local flow = Flow.new("send_email_resend")

-- Step 1: Send a welcome email via Resend
flow:step("send_welcome", nodes.send_email({
    -- Send to yourself for testing; set SENDER_EMAIL in .env
    to = env("SENDER_EMAIL"),
    subject = "Welcome to IronFlow!",
    html = "<h1>Welcome!</h1><p>Thanks for trying IronFlow workflow engine.</p>",
    text = "Welcome! Thanks for trying IronFlow workflow engine.",
    output_key = "email"
}))

-- Step 2: Log the result
flow:step("log", nodes.log({
    level = "info",
    message = "Email sent: success=${ctx.email_success}, data=${ctx.email_data}"
})):depends_on("send_welcome")

return flow
