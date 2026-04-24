use anyhow::{Result, bail};
use async_trait::async_trait;
use tokio::io::AsyncReadExt;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

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

    async fn execute(&self, config: &serde_json::Value, _ctx: &Context) -> Result<NodeOutput> {
        let cmd = config
            .get("cmd")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("shell_command requires 'cmd' parameter"))?;

        let args: Vec<&str> = config
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let cwd = config.get("cwd").and_then(|v| v.as_str());

        let timeout_s = config
            .get("timeout")
            .and_then(|v| v.as_f64())
            .unwrap_or(60.0);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("shell");

        let mut command = tokio::process::Command::new(cmd);
        command.args(&args);

        if let Some(dir) = cwd {
            command.current_dir(dir);
        }

        // Add environment variables from config
        if let Some(env_map) = config.get("env").and_then(|v| v.as_object()) {
            for (k, v) in env_map {
                if let Some(val) = v.as_str() {
                    command.env(k, val);
                }
            }
        }

        // Spawn in a new process group so we can kill the entire tree on timeout
        #[cfg(unix)]
        {
            unsafe {
                command.pre_exec(|| {
                    // Create a new process group with this child as the leader
                    libc::setpgid(0, 0);
                    Ok(())
                });
            }
        }

        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = command.spawn()?;

        // Record the PID before consuming the child.
        // On Unix this is the process group ID (since we called setpgid(0,0)).
        #[cfg(unix)]
        let child_pid = child.id();

        let duration = std::time::Duration::from_secs_f64(timeout_s);
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
                    #[cfg(unix)]
                    if let Some(pid) = child_pid {
                        unsafe {
                            libc::kill(-(pid as i32), libc::SIGKILL);
                        }
                        loop {
                            let ret = unsafe {
                                libc::waitpid(-(pid as i32), std::ptr::null_mut(), libc::WNOHANG)
                            };
                            if ret <= 0 {
                                break;
                            }
                        }
                    }
                    bail!(
                        "Command '{}' timed out after {}s (process group killed)",
                        cmd,
                        timeout_s
                    );
                }
            };

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

        if !success {
            bail!("Command '{}' exited with code {}", cmd, code);
        }

        Ok(result)
    }
}
