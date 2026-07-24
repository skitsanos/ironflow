use anyhow::{Result, bail};
use async_trait::async_trait;
use tokio::io::AsyncReadExt;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::child_process::{ChildProcessGuard, configure_command};
use crate::nodes::{Node, NodeFailure};
use crate::util::duration::positive_duration;
use crate::util::node_config::{config_bool_or, config_f64_or};

fn resolve_command(config: &serde_json::Value, ctx: &Context) -> Result<String> {
    config
        .get("cmd")
        .and_then(serde_json::Value::as_str)
        .map(|value| interpolate_ctx(value, ctx))
        .ok_or_else(|| anyhow::anyhow!("shell_command requires 'cmd' parameter"))
}

fn resolve_arguments(config: &serde_json::Value, ctx: &Context) -> Result<Vec<String>> {
    let Some(value) = config.get("args") else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("shell_command expects 'args' to be an array"))?;

    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(|value| interpolate_ctx(value, ctx))
                .ok_or_else(|| {
                    anyhow::anyhow!("shell_command expects every 'args' entry to be a string")
                })
        })
        .collect()
}

fn resolve_working_directory(config: &serde_json::Value, ctx: &Context) -> Result<Option<String>> {
    config
        .get("cwd")
        .map(|value| {
            value
                .as_str()
                .map(|value| interpolate_ctx(value, ctx))
                .ok_or_else(|| anyhow::anyhow!("shell_command expects 'cwd' to be a string"))
        })
        .transpose()
}

fn resolve_environment(config: &serde_json::Value, ctx: &Context) -> Result<Vec<(String, String)>> {
    let Some(value) = config.get("env") else {
        return Ok(Vec::new());
    };
    let values = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("shell_command expects 'env' to be an object"))?;

    values
        .iter()
        .map(|(name, value)| {
            value
                .as_str()
                .map(|value| (name.clone(), interpolate_ctx(value, ctx)))
                .ok_or_else(|| {
                    anyhow::anyhow!("shell_command expects every 'env' value to be a string")
                })
        })
        .collect()
}

/// Read up to `limit + 1` bytes from a child pipe into `buf`. Returns whether
/// the cap was exceeded. The extra byte is needed to distinguish "at limit"
/// from "over limit"; we keep only `limit` in the buffer either way.
async fn read_capped<R>(mut reader: R, buf: &mut Vec<u8>, limit: usize) -> std::io::Result<bool>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut tmp = [0u8; 8192];
    loop {
        let n = reader.read(&mut tmp).await?;
        if n == 0 {
            return Ok(false);
        }
        let remaining = limit.saturating_sub(buf.len());
        if n > remaining {
            buf.extend_from_slice(&tmp[..remaining]);
            // Drain the rest so the child's pipe doesn't back up. We don't
            // keep the overflow, just ensure the child can continue and exit.
            let mut sink = [0u8; 8192];
            while reader.read(&mut sink).await? != 0 {}
            return Ok(true);
        }
        buf.extend_from_slice(&tmp[..n]);
    }
}

pub struct ShellCommandNode;

#[async_trait]
impl Node for ShellCommandNode {
    fn node_type(&self) -> &str {
        "shell_command"
    }

    fn description(&self) -> &str {
        "Execute a shell command and capture output"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let cmd = resolve_command(config, ctx)?;
        let args = resolve_arguments(config, ctx)?;
        let cwd = resolve_working_directory(config, ctx)?;
        let environment = resolve_environment(config, ctx)?;

        let timeout_s = config_f64_or(config, "timeout", ctx, 60.0)?;
        let duration = positive_duration(timeout_s, "shell_command timeout")?;
        let fail_on_nonzero = config_bool_or(config, "fail_on_nonzero", ctx, true)?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("shell");

        let mut command = tokio::process::Command::new(&cmd);
        command.args(&args);

        if let Some(dir) = &cwd {
            command.current_dir(dir);
        }

        for (name, value) in environment {
            command.env(name, value);
        }

        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        configure_command(&mut command);

        let mut child = command.spawn()?;
        let process_guard = ChildProcessGuard::new(&child);

        let max_out = crate::util::limits::max_shell_output_bytes() as usize;

        // Stream stdout/stderr concurrently with bounded buffers so the
        // child's pipe never forces us to buffer more than `max_out` bytes
        // per stream in memory. The pipe is still drained to avoid deadlock.
        let stdout_pipe = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("shell_command: failed to capture stdout"))?;
        let stderr_pipe = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("shell_command: failed to capture stderr"))?;

        let stdout_fut = async move {
            let mut buf = Vec::new();
            let truncated = read_capped(stdout_pipe, &mut buf, max_out).await?;
            std::io::Result::Ok((buf, truncated))
        };
        let stderr_fut = async move {
            let mut buf = Vec::new();
            let truncated = read_capped(stderr_pipe, &mut buf, max_out).await?;
            std::io::Result::Ok((buf, truncated))
        };

        let combined = async {
            let (stdout_res, stderr_res, wait_res) =
                tokio::join!(stdout_fut, stderr_fut, child.wait());
            Ok::<_, anyhow::Error>((stdout_res?, stderr_res?, wait_res?))
        };

        let ((stdout_bytes, stdout_truncated), (stderr_bytes, stderr_truncated), status) =
            match tokio::time::timeout(duration, combined).await {
                Ok(Ok(x)) => x,
                Ok(Err(e)) => bail!("Failed to execute command '{}': {:#}", cmd, e),
                Err(_) => {
                    process_guard.terminate(&mut child).await;
                    bail!(
                        "Command '{}' timed out after {}s (process terminated)",
                        cmd,
                        timeout_s
                    );
                }
            };
        process_guard.disarm();

        let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
        let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
        let code = status.code().unwrap_or(-1);
        let success = status.success();

        let mut result = NodeOutput::new();
        result.insert(
            format!("{}_stdout", output_key),
            serde_json::Value::String(stdout),
        );
        result.insert(
            format!("{}_stderr", output_key),
            serde_json::Value::String(stderr),
        );
        result.insert(
            format!("{}_code", output_key),
            serde_json::Value::Number(code.into()),
        );
        result.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(success),
        );
        if stdout_truncated || stderr_truncated {
            result.insert(
                format!("{}_output_truncated", output_key),
                serde_json::Value::Bool(true),
            );
        }

        if fail_on_nonzero && !success {
            return Err(NodeFailure::new(
                format!("Command '{cmd}' exited with code {code}"),
                result,
            )
            .into());
        }

        Ok(result)
    }
}
