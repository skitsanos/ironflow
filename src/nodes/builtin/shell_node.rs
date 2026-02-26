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

        let child = command.spawn()?;

        // Record the PID before consuming the child via wait_with_output.
        // On Unix this is the process group ID (since we called setpgid(0,0)).
        #[cfg(unix)]
        let child_pid = child.id();

        let duration = std::time::Duration::from_secs_f64(timeout_s);

        // wait_with_output reads stdout/stderr concurrently while waiting for
        // the process to exit, preventing pipe-buffer deadlocks.
        let result = tokio::time::timeout(duration, child.wait_with_output()).await;

        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => bail!("Failed to execute command '{}': {}", cmd, e),
            Err(_) => {
                // Timeout expired — kill the entire process group and reap
                #[cfg(unix)]
                if let Some(pid) = child_pid {
                    // Negative PID signals the whole process group
                    unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
                    // Reap all children in the process group to prevent zombies.
                    // waitpid(-pgid, WNOHANG) in a loop until no more remain.
                    loop {
                        let ret = unsafe {
                            libc::waitpid(-(pid as i32), std::ptr::null_mut(), libc::WNOHANG)
                        };
                        if ret <= 0 {
                            break;
                        }
                    }
                }
                bail!("Command '{}' timed out after {}s (process group killed)", cmd, timeout_s);
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);
        let success = output.status.success();

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
