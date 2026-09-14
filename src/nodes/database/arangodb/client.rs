use std::time::Duration;

use anyhow::{Result, bail};
use reqwest::{Method, RequestBuilder, StatusCode, Url};
use serde_json::Value;

use super::config::{Action, Operation, resolve_param};
use crate::engine::types::Context;
use crate::util::duration::positive_duration;
use crate::util::node_config::config_f64_or;
use crate::util::sensitive_url::{SecretEndpoint, redact_sensitive_text};

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) struct CursorClient {
    http: reqwest::Client,
    url: Url,
    token: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

impl CursorClient {
    pub(super) fn new(config: &Value, ctx: &Context) -> Result<Self> {
        let url = resolve_param(config, "url", "ARANGODB_URL", ctx).ok_or_else(|| {
            anyhow::anyhow!("arangodb_aql requires 'url' or ARANGODB_URL env var")
        })?;
        let database =
            resolve_param(config, "database", "ARANGODB_DATABASE", ctx).ok_or_else(|| {
                anyhow::anyhow!("arangodb_aql requires 'database' or ARANGODB_DATABASE env var")
            })?;
        if database.is_empty() || database == "." || database == ".." {
            bail!("arangodb_aql requires a non-empty database name other than . or ..");
        }
        let mut url = Url::parse(&url)
            .map_err(|_| anyhow::anyhow!("arangodb_aql requires a valid HTTP(S) URL"))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            bail!("arangodb_aql requires a valid HTTP(S) URL");
        }
        url.set_fragment(None);
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid ArangoDB URL path"))?
            .pop_if_empty()
            .extend(["_db", &database, "_api", "cursor"]);
        let timeout = positive_duration(
            config_f64_or(config, "timeout", ctx, 30.0)?,
            "arangodb_aql timeout",
        )?;
        let http = crate::util::provider_http::client_builder()
            .timeout(timeout)
            .build()
            .map_err(|error| {
                anyhow::anyhow!(
                    "Failed to build ArangoDB client: {}",
                    redact_sensitive_text(&error.to_string())
                )
            })?;
        Ok(Self {
            http,
            url,
            token: resolve_param(config, "token", "ARANGODB_TOKEN", ctx),
            username: resolve_param(config, "username", "ARANGODB_USERNAME", ctx),
            password: resolve_param(config, "password", "ARANGODB_PASSWORD", ctx),
        })
    }

    fn request(&self, method: Method, cursor: Option<&str>) -> RequestBuilder {
        let mut url = self.url.clone();
        if let Some(cursor) = cursor {
            url.path_segments_mut()
                .expect("validated HTTP URL")
                .push(cursor);
        }
        let request = self.http.request(method, url);
        if let Some(token) = &self.token {
            request.bearer_auth(token)
        } else if let Some(username) = &self.username {
            request.basic_auth(username, self.password.as_ref())
        } else {
            request
        }
    }

    pub(super) async fn execute(&self, operation: &Operation) -> Result<(StatusCode, Value)> {
        let method = if operation.action == Action::Close {
            Method::DELETE
        } else {
            Method::POST
        };
        let mut request = self.request(method, operation.cursor_id.as_deref());
        if let Some(body) = &operation.body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|error| {
            anyhow::anyhow!(
                "ArangoDB request to {} failed: {}",
                SecretEndpoint::new(self.url.as_str()),
                redact_sensitive_text(&error.to_string())
            )
        })?;
        let status = response.status();
        let body = read_json(response, crate::util::limits::max_http_body_bytes()).await?;
        Ok((status, body))
    }

    pub(super) fn cleanup(&self, cursor: Option<&str>) -> CursorCleanup {
        CursorCleanup(cursor.map(|id| {
            self.request(Method::DELETE, Some(id))
                .timeout(CLEANUP_TIMEOUT)
        }))
    }
}

async fn read_json(mut response: reqwest::Response, maximum: u64) -> Result<Value> {
    if response.content_length().is_some_and(|n| n > maximum) {
        bail!("ArangoDB response exceeds IRONFLOW_MAX_HTTP_BODY_BYTES ({maximum})");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        anyhow::anyhow!(
            "Failed to read ArangoDB response: {}",
            redact_sensitive_text(&error.to_string())
        )
    })? {
        if chunk.len() as u64 > maximum.saturating_sub(bytes.len() as u64) {
            bail!("ArangoDB response exceeds IRONFLOW_MAX_HTTP_BODY_BYTES ({maximum})");
        }
        bytes
            .try_reserve_exact(chunk.len())
            .map_err(|_| anyhow::anyhow!("Cannot reserve ArangoDB response buffer"))?;
        bytes.extend_from_slice(&chunk);
        tokio::task::yield_now().await;
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Failed to parse ArangoDB response JSON"))
}

pub(super) fn check_response(status: StatusCode, body: &Value, closing: bool) -> Result<()> {
    if closing
        && status == StatusCode::NOT_FOUND
        && body.get("error") == Some(&Value::Bool(true))
        && body.get("errorNum").and_then(Value::as_i64) == Some(1600)
    {
        return Ok(());
    }
    if !status.is_success() || body.get("error") == Some(&Value::Bool(true)) {
        let message = body
            .get("errorMessage")
            .and_then(Value::as_str)
            .unwrap_or("Unknown error");
        let number = body.get("errorNum").and_then(Value::as_i64).unwrap_or(0);
        bail!(
            "ArangoDB error {number}: {} (HTTP {status})",
            redact_sensitive_text(message)
        );
    }
    if !body.is_object() || body.get("error").is_some_and(|e| e != &Value::Bool(false)) {
        bail!("Invalid ArangoDB response envelope");
    }
    if closing && status != StatusCode::ACCEPTED {
        bail!("ArangoDB cursor close requires HTTP 202 or cursor-not-found 404");
    }
    Ok(())
}

pub(super) struct CursorCleanup(Option<RequestBuilder>);

impl CursorCleanup {
    pub(super) fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for CursorCleanup {
    fn drop(&mut self) {
        let Some(request) = self.0.take() else {
            return;
        };
        // Cancellation cannot await. One bounded best-effort DELETE releases a
        // known cursor; unknown IDs and runtime/process shutdown still rely on TTL.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if !matches!(request.send().await, Ok(response) if response.status().is_success() || response.status() == StatusCode::NOT_FOUND) {
                    tracing::warn!("ArangoDB cursor cleanup failed; server TTL remains the fallback");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests;
