# `shell_command`

Execute a shell command and capture its output.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `cmd` | string | yes | — | The command to execute. |
| `args` | array | no | `[]` | List of string arguments passed to the command. |
| `cwd` | string | no | *(process cwd)* | Working directory for the command. When omitted the engine's current directory is used. |
| `env` | table | no | `{}` | Additional environment variables injected into the child process. Each key/value must be a string. |
| `timeout` | number | no | `60` | Maximum execution time in seconds (supports decimals). If the timeout expires the entire process group is killed. |
| `output_key` | string | no | `"shell"` | Prefix used for the context keys written by this node. |

## Context Output

- `{output_key}_stdout` — Standard output of the command (string).
- `{output_key}_stderr` — Standard error of the command (string).
- `{output_key}_code` — Exit code (number). Returns `-1` when the code is unavailable.
- `{output_key}_success` — `true` when the process exits with code 0, `false` otherwise.

> **Note:** If the command exits with a non-zero code the node raises an error after writing the output keys to the context.

## Example

```lua
local flow = Flow.new("shell_demo")

flow:step("whoami", nodes.shell_command({
    cmd = "whoami",
    output_key = "user"
}))

flow:step("disk_usage", nodes.shell_command({
    cmd = "df",
    args = { "-h", "/" },
    timeout = 10,
    output_key = "disk"
}))

flow:step("echo_env", nodes.shell_command({
    cmd = "sh",
    args = { "-c", "echo \"Hello $GREETING_NAME\"" },
    env = {
        GREETING_NAME = "IronFlow"
    },
    output_key = "echo"
}))

flow:step("summary", nodes.log({
    message = "User: ${ctx.user_stdout}, Echo: ${ctx.echo_stdout}",
    level = "info"
})):depends_on("whoami", "disk_usage", "echo_env")

return flow
```
