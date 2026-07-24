# `shell_command`

Execute a shell command and capture its output.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `cmd` | string | yes | — | The command to execute. Supports context interpolation. |
| `args` | array | no | `[]` | List of string arguments passed to the command. Each value supports context interpolation. |
| `cwd` | string | no | *(process cwd)* | Working directory for the command. Supports context interpolation; when omitted the engine's current directory is used. |
| `env` | table | no | `{}` | Additional environment variables injected into the child process. Each value must be a string and supports context interpolation; keys are literal. |
| `timeout` | number | no | `60` | Node-local maximum execution time in seconds (supports decimals). The shorter enclosing step timeout wins. |
| `output_key` | string | no | `"shell"` | Prefix used for the context keys written by this node. |
| `fail_on_nonzero` | boolean | no | `true` | Fail the task when a completed process exits unsuccessfully. Set to `false` to inspect the exit status as normal node output. Supports context interpolation. |

## Context Output

- `{output_key}_stdout` — Standard output of the command (string).
- `{output_key}_stderr` — Standard error of the command (string).
- `{output_key}_code` — Exit code (number). Returns `-1` when the code is unavailable.
- `{output_key}_success` — `true` when the process exits with code 0, `false` otherwise.
- `{output_key}_output_truncated` — Optional `true` when either captured stream exceeded its per-stream size cap. The key is absent when neither stream was truncated.

Stdout and stderr are decoded with lossy UTF-8 conversion. Each stream is
captured independently up to `IRONFLOW_MAX_SHELL_OUTPUT_BYTES` (10 MiB by
default), and the remainder is drained without being retained.

## Exit and failure policy

| Process outcome | `fail_on_nonzero = true` | `fail_on_nonzero = false` |
|---|---|---|
| Completed successfully | Task succeeds and all output keys are published. | Same. |
| Completed unsuccessfully | Task fails and is eligible for step retries. Only the terminal failed attempt's output is published and persisted. | Task succeeds on that attempt with `{output_key}_success = false`; step retries and `on_error` are not triggered. |
| Config, spawn, pipe I/O, wait, cancellation, or timeout failure | Task fails without completed-process output. | Same; the flag does not hide operational failures. |

The default preserves fail-fast behavior while retaining diagnostics. Failed
attempt output stays private during retries, so a later attempt cannot observe
or inherit it. If retries are exhausted, the final structured output is merged
into workflow context at the source phase barrier and stored in the failed
task's `output`. A successful retry publishes only its successful output. If
the total step deadline expires during retry backoff, the last completed
attempt is the terminal diagnostic; an execution timeout has no
completed-process output.

An `on_error` handler can read the normal output keys and an exact
invocation-local copy in `ctx._error_output`. The private `_error_output` object
identifies this failed task's exact output even if a later-declared parallel
step wins a shared-context collision. It is not added to shared context unless
the handler explicitly returns it. Task history still obeys
`IRONFLOW_MAX_TASK_OUTPUT_BYTES`; when its copy is replaced by a truncation
marker, a recovery handler still receives the exact redacted fields through
`_error_output`; their normal shared keys remain subject to declaration-order
collision precedence.

The child is terminated when the node-local timeout expires, the enclosing
step times out, or the workflow is explicitly cancelled. IronFlow guarantees
direct-child termination on every platform. On Unix the command starts as a
process-group leader and the inherited group is killed as well; a descendant
that explicitly daemonizes into another session/process group can escape. A
command side effect completed before termination is not rolled back.

Interpolation uses the shared path grammar, including zero-based array indexes
such as `${ctx.items[0].name}`. Shell-style forms outside the reserved `ctx`
namespace, such as `${HOME}` and `${TMPDIR:-/tmp}`, remain unchanged so a shell
can expand them. Use `\${ctx.foo}` at runtime (Lua `"\\${ctx.foo}"`) when the
shell itself must receive a literal `${ctx.foo}`.

## Example

```lua
-- Run with --context '{"request":{"region":"eu-west-1"}}'.
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
    message = "User: ${ctx.user_stdout}, Echo: ${ctx.echo_stdout}, Probe exit: ${ctx.probe_code}",
    level = "info"
})):depends_on("whoami", "disk_usage", "echo_env", "status_probe")

return flow
```
