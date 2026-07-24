-- Demonstrates shell command execution
-- Platform: requires a POSIX-like environment with `whoami`, `df`, and `sh`
-- available on PATH.
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

-- Expected unsuccessful statuses can be inspected without turning the shell
-- step into a workflow failure. Operational failures and timeouts still fail.
flow:step("status_probe", nodes.shell_command({
    cmd = "sh",
    args = { "-c", "printf 'service not ready' >&2; exit 7" },
    fail_on_nonzero = false,
    output_key = "probe"
}))

-- Log results (all four run in parallel, then this runs)
flow:step("summary", nodes.log({
    message = "User: ${ctx.user_stdout}, Echo: ${ctx.echo_stdout}, Probe exit: ${ctx.probe_code}, Probe success: ${ctx.probe_success}",
    level = "info"
})):depends_on("whoami", "disk_usage", "echo_env", "status_probe")

return flow

-- Run with:
--   ironflow run examples/06-shell/run_commands.lua --verbose
