use std::collections::VecDeque;
use std::time::Duration;

use tokio::io::{BufReader, Lines};
use tokio::process::{Child, ChildStderr};

fn record_line(tail: &mut VecDeque<String>, line: &str) {
    let redacted = ironflow::util::sensitive_url::redact_sensitive_text(line);
    tail.push_back(redacted.chars().take(1024).collect());
    if tail.len() > 16 {
        tail.pop_front();
    }
}

pub async fn wait_for_listener(
    child: &mut Child,
    stderr: &mut Lines<BufReader<ChildStderr>>,
    deadline: Duration,
) -> Result<String, String> {
    let mut tail = VecDeque::new();
    let startup = tokio::time::timeout(deadline, async {
        while let Some(line) = stderr
            .next_line()
            .await
            .map_err(|error| format!("cannot read server stderr: {error}"))?
        {
            record_line(&mut tail, &line);
            if let Some((_, address)) = line.split_once("IronFlow API server listening on ") {
                let address: String = address
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ':'))
                    .collect();
                address
                    .parse::<std::net::SocketAddr>()
                    .map_err(|_| "server reported an invalid listening address".to_string())?;
                return Ok(format!("http://{address}"));
            }
        }
        Err("server exited before listening".to_string())
    })
    .await;
    let reason = match startup {
        Ok(Ok(base)) => return Ok(base),
        Ok(Err(reason)) => reason,
        Err(_) => format!("server startup timed out after {}ms", deadline.as_millis()),
    };

    // A failed startup must not leave a serving process or a zombie behind.
    let _ = child.start_kill();
    let status = match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(Ok(status)) => match status.code() {
            Some(code) => format!("exit code {code}"),
            None => status.to_string(),
        },
        Ok(Err(error)) => format!("cannot reap server: {error}"),
        Err(_) => "server did not exit after kill".to_string(),
    };
    let logs = tail.into_iter().collect::<Vec<_>>().join("\n");
    Err(format!("{reason}; {status}; stderr tail:\n{logs}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Connection as _;
    use std::process::Stdio;
    use tokio::io::AsyncBufReadExt as _;

    #[test]
    fn diagnostics_keep_a_bounded_utf8_tail() {
        let mut tail = VecDeque::new();
        for index in 0..100 {
            record_line(&mut tail, &format!("{index}: {}", "x".repeat(2048)));
        }
        assert_eq!(tail.len(), 16);
        assert!(tail.front().unwrap().starts_with("84:"));
        assert!(tail.iter().all(|line| line.chars().count() == 1024));
        record_line(&mut tail, &"\u{e9}".repeat(2048));
        assert_eq!(tail.back().unwrap().chars().count(), 1024);
        record_line(&mut tail, "final startup error");
        assert_eq!(tail.back().unwrap(), "final startup error");
    }

    #[test]
    fn diagnostics_redact_credentials_before_retaining_them() {
        let mut tail = VecDeque::new();
        record_line(
            &mut tail,
            "connect postgres://fixture:startup-password@localhost/db?token=startup-token",
        );
        let diagnostic = tail.back().unwrap();
        assert!(!diagnostic.contains("startup-password"));
        assert!(!diagnostic.contains("startup-token"));
        assert!(diagnostic.contains("REDACTED"));
    }

    #[tokio::test]
    async fn a_blocked_sqlite_startup_is_killed_and_reaped_at_the_deadline() {
        let directory = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite:{}?mode=rwc",
            directory.path().join("locked.sqlite").display()
        );
        let mut lock = sqlx::SqliteConnection::connect(&url).await.unwrap();
        sqlx::query("BEGIN EXCLUSIVE")
            .execute(&mut lock)
            .await
            .unwrap();
        let mut command = super::super::command(directory.path());
        command
            .args(["serve", "--host", "127.0.0.1", "--port", "0"])
            .env("IRONFLOW_STORE", "sqlite")
            .env("IRONFLOW_STORE_URL", &url)
            .env("IRONFLOW_ALLOW_UNAUTHENTICATED_API", "true")
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
        let diagnostic = wait_for_listener(&mut child, &mut stderr, Duration::from_millis(100))
            .await
            .unwrap_err();
        assert!(diagnostic.contains("startup timed out"), "{diagnostic}");
        assert!(child.id().is_none(), "failed child must have been reaped");
        sqlx::query("ROLLBACK").execute(&mut lock).await.unwrap();
        lock.close().await.unwrap();
    }
}
