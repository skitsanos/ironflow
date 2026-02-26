use anyhow::{Result, bail};
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct ShellCommandNode;

#[async_trait]
impl Node for ShellCommandNode {
    fn node_type(&self) -> &str {
        "shell_command"
    }

    fn description(&self) -> &str {
        "Execute a shell command and capture output"
    }

    async fn execute(&self, config: &serde_json::Value, _ctx: Context) -> Result<NodeOutput> {
        let cmd = config
            .get("cmd")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("shell_command requires 'cmd' parameter"))?;

        let args: Vec<&str> = config
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect()
            })
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

        let duration = std::time::Duration::from_secs_f64(timeout_s);
        let mut child = command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Take stdout/stderr handles before waiting, so we can still kill on timeout
        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        let wait_result = tokio::time::timeout(duration, child.wait()).await;

        let status = match wait_result {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => bail!("Failed to execute command '{}': {}", cmd, e),
            Err(_) => {
                // Timeout expired — kill the child process
                let _ = child.kill().await;
                // Wait to reap the zombie
                let _ = child.wait().await;
                bail!("Command '{}' timed out after {}s (process killed)", cmd, timeout_s);
            }
        };

        // Read captured output
        let stdout = if let Some(mut out) = stdout_handle {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).to_string()
        } else {
            String::new()
        };

        let stderr = if let Some(mut err) = stderr_handle {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).to_string()
        } else {
            String::new()
        };

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

        if !success {
            bail!(
                "Command '{}' exited with code {}",
                cmd,
                code
            );
        }

        Ok(result)
    }
}
