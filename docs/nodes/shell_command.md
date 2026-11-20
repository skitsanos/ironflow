# `shell_command`

Run a program with arguments and capture its exit code, stdout and stderr.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `cmd` | string | Yes | — | Program to run; supports `${ctx.*}` interpolation. |
| `args` | array of strings | No | `[]` | Arguments passed to the program; each supports interpolation. |
| `cwd` | string | No | process directory | Working directory; supports interpolation. |
| `env` | object | No | `{}` | Extra environment variables; values must be strings and support interpolation, keys are literal. |
| `timeout` | number | No | `60` | Seconds before the process is killed; must be positive. A shorter step timeout wins. |
| `output_key` | string | No | `"shell"` | Prefix for the output keys. |
| `fail_on_nonzero` | boolean | No | `true` | Fail the step when the process exits unsuccessfully; `false` returns the exit status as normal output. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_stdout` | string | Standard output, decoded as lossy UTF-8. |
| `{output_key}_stderr` | string | Standard error, decoded as lossy UTF-8. |
| `{output_key}_code` | number | Exit code, or `-1` when none is available (for example, killed by a signal). |
| `{output_key}_success` | boolean | `true` when the process exited successfully. |
| `{output_key}_output_truncated` | boolean | `true` when a stream exceeded the capture limit; absent otherwise. |

## Behavior

- `cmd` is run directly, not through a shell: pipes, redirects and `$VAR` need `cmd = "sh", args = { "-c", "..." }`.
- With `fail_on_nonzero = true`, a non-zero exit fails the step (retries apply) and the output keys of the last attempt are still published; an `on_error` handler also receives them in `ctx._error_output`.
- With `fail_on_nonzero = false`, a non-zero exit succeeds with `{output_key}_success = false`; retries and `on_error` do not run.
- A spawn failure, I/O error, timeout or cancellation always fails the step, with no output keys.
- On timeout or cancellation the process is killed; on Unix its process group is killed too. Side effects already done are not undone.
- Interpolation accepts the full path syntax (`${ctx.items[0].name}`). Placeholders outside `ctx`, such as `${HOME}`, are left for the shell; write `\${ctx.foo}` (Lua `"\\${ctx.foo}"`) to pass a literal `${ctx.foo}`.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_SHELL_OUTPUT_BYTES` | `10485760` | Bytes captured per stream; the rest is read and discarded and `{output_key}_output_truncated` is set. |

## Examples

### Run commands and inspect exit status

```lua
-- Run with --context '{"request":{"region":"eu-west-1"}}'.
local flow = Flow.new("shell_demo")

flow:step("disk_usage", nodes.shell_command({
    cmd = "df",
    args = { "-h", "/" },
    timeout = 10,
    output_key = "disk"
}))

flow:step("echo_env", nodes.shell_command({
    cmd = "sh",
    args = { "-c", "echo \"Hello $GREETING_NAME from ${ctx.request.region}\"" },
    env = {
        GREETING_NAME = "IronFlow"
    },
    output_key = "echo"
}))

-- Inspect an expected unsuccessful status without failing the step.
flow:step("status_probe", nodes.shell_command({
    cmd = "sh",
    args = { "-c", "printf 'not ready' >&2; exit 7" },
    fail_on_nonzero = false,
    output_key = "probe"
}))

flow:step("summary", nodes.log({
    message = "Echo: ${ctx.echo_stdout}, Probe exit: ${ctx.probe_code}",
    level = "info"
})):depends_on("disk_usage", "echo_env", "status_probe")

return flow
```
