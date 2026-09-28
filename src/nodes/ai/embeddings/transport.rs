use std::time::Duration;

use anyhow::{Result, ensure};

use super::response::{parse_ollama_response, parse_openai_response};

pub(super) struct Endpoint<'a> {
    pub client: &'a reqwest::Client,
    pub url: String,
    pub api_key: Option<&'a str>,
    pub model: &'a str,
}

impl Endpoint<'_> {
    pub(super) async fn request(&self, texts: &[String], retries: usize) -> Result<Vec<Vec<f64>>> {
        let future = self.request_with_retries(texts, retries);
        match crate::util::execution::current_execution_deadline() {
            Some(deadline) => {
                ensure!(
                    tokio::time::Instant::now() < deadline,
                    "embedding step deadline exceeded"
                );
                tokio::time::timeout_at(deadline, future)
                    .await
                    .map_err(|_| anyhow::anyhow!("embedding step deadline exceeded"))?
            }
            None => future.await,
        }
    }

    async fn request_with_retries(
        &self,
        texts: &[String],
        retries: usize,
    ) -> Result<Vec<Vec<f64>>> {
        for attempt in 0..=retries {
            let mut request = self
                .client
                .post(&self.url)
                .json(&serde_json::json!({"model": self.model, "input": texts}));
            if let Some(key) = self.api_key {
                request = request.bearer_auth(key);
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    let transient = error.is_timeout() || error.is_connect();
                    if transient && attempt < retries {
                        tokio::time::sleep(backoff(attempt)).await;
                        continue;
                    }
                    // Reqwest's URL and provider bodies can contain private inputs or credentials.
                    let kind = if error.is_redirect() {
                        "redirect refused"
                    } else if error.is_timeout() {
                        "request timed out"
                    } else if error.is_connect() {
                        "connection failed"
                    } else {
                        "request failed"
                    };
                    anyhow::bail!("embedding {kind} after {} attempt(s)", attempt + 1);
                }
            };
            let status = response.status();
            if !status.is_success() {
                let transient = matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504);
                if transient && attempt < retries {
                    let delay = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(retry_after)
                        .unwrap_or_else(|| backoff(attempt))
                        .max(Duration::from_millis(10));
                    ensure!(
                        delay <= Duration::from_secs(60),
                        "embedding HTTP {status}: Retry-After exceeds 60-second batch retry budget"
                    );
                    drop(response);
                    tokio::time::sleep(delay).await;
                    continue;
                }
                anyhow::bail!(
                    "embedding HTTP {status} after {} attempt(s); provider body omitted",
                    attempt + 1
                );
            }
            let body = crate::util::provider_http::bounded_response_text(response).await?;
            return if self.api_key.is_some() {
                parse_openai_response(status, &body)
            } else {
                parse_ollama_response(status, &body)
            };
        }
        unreachable!("last attempt returns its result")
    }
}

fn backoff(attempt: usize) -> Duration {
    Duration::from_millis(500 * (1 << attempt))
}

fn retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    Some(
        (at.with_timezone(&chrono::Utc) - chrono::Utc::now())
            .to_std()
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_accepts_seconds_and_http_dates() {
        assert_eq!(retry_after("7"), Some(Duration::from_secs(7)));
        assert_eq!(
            retry_after("Wed, 21 Oct 2015 07:28:00 GMT"),
            Some(Duration::ZERO)
        );
        assert_eq!(retry_after("invalid"), None);
        assert_eq!(retry_after("-1"), None);
    }
}
