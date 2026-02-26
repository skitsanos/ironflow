-- Demonstrates shell command execution
local flow = Flow.new("shell_commands")

-- Run a simple command
flow:step("whoami", nodes.shell_command({
    cmd = "whoami",
    output_key = "user"
}))

-- Run with arguments and capture output
flow:step("disk_usage", nodes.shell_command({
    cmd = "df",
    args = { "-h", "/" },
    timeout = 10,
    output_key = "disk"
}))

-- Run with environment variables
flow:step("echo_env", nodes.shell_command({
    cmd = "sh",
    args = { "-c", "echo \"Hello $GREETING_NAME from $GREETING_SOURCE\"" },
    env = {
        GREETING_NAME = "IronFlow",
        GREETING_SOURCE = "shell_command node"
    },
    output_key = "echo"
}))

-- Log results (all three run in parallel, then this runs)
flow:step("summary", nodes.log({
    message = "User: ${ctx.user_stdout}, Echo: ${ctx.echo_stdout}",
    level = "info"
})):depends_on("whoami", "disk_usage", "echo_env")

return flow

-- Run with:
--   ironflow run examples/06-shell/run_commands.lua --verbose
