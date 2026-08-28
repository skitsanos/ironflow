mod body;
mod builder;
mod dns;
mod nodes;
mod response;
mod transport;

use anyhow::Result;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::util::duration::{nonnegative_duration, positive_duration};
use crate::util::node_config::{
    config_bool_or, config_f64_or, config_u64_strict, config_usize_strict,
};
use crate::util::sensitive_url::SecretEndpoint;

use body::RequestBody;
pub use nodes::{HttpDeleteNode, HttpGetNode, HttpPostNode, HttpPutNode, HttpRequestNode};
use response::{ResponseMetadata, ResponseMode, response_to_output};
use transport::{ProxyMode, RedirectPolicy, send_with_redirects, shared_client};

/// Status retries are nested inside a step attempt, so leaving this count
/// unbounded can multiply the workflow retry ceiling into an effectively
/// unlimited request storm.
const MAX_STATUS_RETRIES: u64 = 100;

/// Redirects are another nested request loop and need an independent ceiling.
const MAX_REDIRECTS: usize = 100;

/// Even when a provider sends `Retry-After: 0`, yield for a small bounded
/// interval instead of spinning a tight network loop.
const MIN_STATUS_RETRY_DELAY_SECONDS: f64 = 0.01;

pub(super) async fn do_http_request(
    method: &str,
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<NodeOutput> {
    let raw_url = config
        .get("url")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("HTTP node requires 'url' parameter"))?;
    let url = interpolate_ctx(raw_url, ctx);
    let request_body = RequestBody::resolve(config, ctx)?;

    let timeout_s = config_f64_or(config, "timeout", ctx, 30.0)?;
    let timeout = positive_duration(timeout_s, "HTTP timeout")?;
    let output_key = config
        .get("output_key")
        .and_then(|value| value.as_str())
        .unwrap_or("http");
    let response_mode = ResponseMode::parse(config)?;
    let fail_on_status = config_bool_or(config, "fail_on_status", ctx, true)?;
    let retry_statuses = parse_retry_statuses(config, ctx)?;
    let status_retries = parse_status_retries(config, ctx)?;
    let retry_backoff_s = config_f64_or(config, "status_retry_backoff", ctx, 1.0)?;
    nonnegative_duration(retry_backoff_s, "HTTP status_retry_backoff")?;
    let respect_retry_after = config_bool_or(config, "respect_retry_after", ctx, true)?;
    let max_retry_after_s = config_f64_or(config, "max_retry_after", ctx, 60.0)?;
    nonnegative_duration(max_retry_after_s, "HTTP max_retry_after")?;

    if status_retries > 0 {
        validate_retry_delay("status_retry_backoff", retry_backoff_s)?;
        validate_retry_delay("max_retry_after", max_retry_after_s)?;
    }

    // SSRF controls. `max_redirects` bounds (or disables) redirect following;
    // `block_private_network` additionally refuses the initial URL and any
    // redirect hop that targets the local host or a private network.
    let max_redirects = config_usize_strict(config, "max_redirects", ctx)?.unwrap_or(10);
    if max_redirects > MAX_REDIRECTS {
        anyhow::bail!("HTTP redirect count is too large (max {MAX_REDIRECTS})");
    }
    let allow_cross_origin_redirects =
        config_bool_or(config, "allow_cross_origin_redirects", ctx, false)?;
    let carries_redirect_sensitive_data =
        carries_redirect_sensitive_data(config, &url, request_body.has_payload());
    // This is a security control: a typo must fail closed rather than silently
    // turning private-network protection off.
    let block_private = config_bool_or(config, "block_private_network", ctx, false)?;

    let client = shared_client(ProxyMode::parse(config)?, block_private)?;
    let redirect_policy = RedirectPolicy {
        max_redirects,
        allow_cross_origin: allow_cross_origin_redirects,
        block_private,
        carries_sensitive_data: carries_redirect_sensitive_data,
    };

    let mut attempt = 0_u64;
    loop {
        let request_attempt = async {
            let response = send_with_redirects(
                &client,
                method,
                &url,
                config,
                ctx,
                &request_body,
                &redirect_policy,
            )
            .await?;
            let metadata = ResponseMetadata::from_response(&response);
            let should_retry = attempt < status_retries
                && retry_statuses.contains(&metadata.status)
                && !metadata.success;
            if should_retry {
                return Ok(RequestAttempt::Retry(metadata.retry_after_secs));
            }
            if fail_on_status && !metadata.success {
                anyhow::bail!(
                    "HTTP {} {} returned status {}",
                    method,
                    SecretEndpoint::new(&url),
                    metadata.status
                );
            }
            let output = response_to_output(response, output_key, response_mode, metadata).await?;
            Ok(RequestAttempt::Complete(output))
        };
        let result = tokio::time::timeout(timeout, request_attempt)
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "HTTP {} request to {} timed out after {} seconds",
                    method,
                    SecretEndpoint::new(&url),
                    timeout.as_secs_f64()
                )
            })??;

        if let RequestAttempt::Retry(retry_after_secs) = result {
            let exponential_backoff =
                || retry_backoff_s * 2_f64.powi(i32::try_from(attempt).unwrap_or(30));
            let delay_s = if respect_retry_after {
                retry_after_secs.unwrap_or_else(exponential_backoff)
            } else {
                exponential_backoff()
            }
            .min(max_retry_after_s)
            .max(MIN_STATUS_RETRY_DELAY_SECONDS);
            attempt += 1;
            if delay_s > 0.0 {
                tokio::time::sleep(nonnegative_duration(delay_s, "HTTP retry delay")?).await;
            }
            continue;
        }

        let RequestAttempt::Complete(mut output) = result else {
            unreachable!("retry attempts continue before output handling")
        };
        output.insert(
            format!("{}_attempts", output_key),
            serde_json::Value::Number((attempt + 1).into()),
        );
        return Ok(output);
    }
}

enum RequestAttempt {
    Retry(Option<f64>),
    Complete(NodeOutput),
}

fn carries_redirect_sensitive_data(
    config: &serde_json::Value,
    request_url: &str,
    has_body: bool,
) -> bool {
    let has_auth = config
        .get("auth")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|auth| !auth.is_empty());
    let has_headers = config
        .get("headers")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|headers| headers.values().any(serde_json::Value::is_string));
    let has_url_credentials = url::Url::parse(request_url)
        .is_ok_and(|url| !url.username().is_empty() || url.password().is_some());
    has_auth || has_headers || has_url_credentials || has_body
}

fn parse_status_retries(config: &serde_json::Value, ctx: &Context) -> Result<u64> {
    let retries = match config_u64_strict(config, "status_retries", ctx)? {
        Some(value) => value,
        None => config_u64_strict(config, "max_status_retries", ctx)?.unwrap_or(0),
    };

    if retries > MAX_STATUS_RETRIES {
        anyhow::bail!(
            "HTTP status retry count is too large (max {MAX_STATUS_RETRIES}); use workflow retries for a separately bounded outer policy"
        );
    }

    Ok(retries)
}

fn validate_retry_delay(name: &str, seconds: f64) -> Result<()> {
    if seconds < MIN_STATUS_RETRY_DELAY_SECONDS {
        anyhow::bail!(
            "HTTP {name} must be at least {MIN_STATUS_RETRY_DELAY_SECONDS} seconds when status retries are enabled"
        );
    }
    Ok(())
}

fn parse_retry_statuses(config: &serde_json::Value, ctx: &Context) -> Result<Vec<u16>> {
    let Some(configured) = config.get("retry_statuses") else {
        return Ok(Vec::new());
    };
    let values = configured
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("'retry_statuses' must be an array of HTTP status codes"))?;
    values
        .iter()
        .map(|value| match value {
            serde_json::Value::Number(number) => number
                .as_u64()
                .and_then(|number| u16::try_from(number).ok())
                .and_then(|number| reqwest::StatusCode::from_u16(number).ok())
                .map(|status| status.as_u16())
                .ok_or_else(|| {
                    anyhow::anyhow!("retry_statuses values must be valid HTTP status codes")
                }),
            serde_json::Value::String(text) => interpolate_ctx(text, ctx)
                .parse::<u16>()
                .ok()
                .and_then(|number| reqwest::StatusCode::from_u16(number).ok())
                .map(|status| status.as_u16())
                .ok_or_else(|| {
                    anyhow::anyhow!("retry_statuses values must be valid HTTP status codes")
                }),
            _ => anyhow::bail!("retry_statuses values must be numbers or numeric strings"),
        })
        .collect()
}
