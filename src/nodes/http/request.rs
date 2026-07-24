mod builder;
mod nodes;
mod response;

use anyhow::Result;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::util::duration::{nonnegative_duration, positive_duration};
use crate::util::node_config::{config_bool, config_f64_or, config_u64};
use crate::util::sensitive_url::{SecretEndpoint, redact_sensitive_text};

use builder::build_request;
pub use nodes::{HttpDeleteNode, HttpGetNode, HttpPostNode, HttpPutNode, HttpRequestNode};
use response::response_to_output;

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

    let timeout_s = config_f64_or(config, "timeout", ctx, 30.0)?;
    let timeout = positive_duration(timeout_s, "HTTP timeout")?;
    let output_key = config
        .get("output_key")
        .and_then(|value| value.as_str())
        .unwrap_or("http");
    let fail_on_status = config_bool(config, "fail_on_status", ctx).unwrap_or(true);
    let retry_statuses = parse_retry_statuses(config, ctx)?;
    let status_retries = config_u64(config, "status_retries", ctx)
        .or_else(|| config_u64(config, "max_status_retries", ctx))
        .unwrap_or(0);
    let retry_backoff_s = config_f64_or(config, "status_retry_backoff", ctx, 1.0)?;
    nonnegative_duration(retry_backoff_s, "HTTP status_retry_backoff")?;
    let respect_retry_after = config_bool(config, "respect_retry_after", ctx).unwrap_or(true);
    let max_retry_after_s = config_f64_or(config, "max_retry_after", ctx, 60.0)?;
    nonnegative_duration(max_retry_after_s, "HTTP max_retry_after")?;

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| {
            anyhow::anyhow!(
                "Failed to build HTTP client: {}",
                redact_sensitive_text(&error.to_string())
            )
        })?;
    let request_template = build_request(&client, method, &url, config, ctx)?
        .try_clone()
        .ok_or_else(|| anyhow::anyhow!("HTTP request body is not retryable"))?;

    let mut attempt = 0_u64;
    loop {
        let response = request_template
            .try_clone()
            .ok_or_else(|| anyhow::anyhow!("HTTP request body is not retryable"))?
            .send()
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "HTTP {} request to {} failed: {}",
                    method,
                    SecretEndpoint::new(&url),
                    redact_sensitive_text(&error.to_string())
                )
            })?;
        let result = response_to_output(response, output_key).await?;
        let should_retry =
            attempt < status_retries && retry_statuses.contains(&result.status) && !result.success;

        if should_retry {
            let exponential_backoff =
                || retry_backoff_s * 2_f64.powi(i32::try_from(attempt).unwrap_or(30));
            let delay_s = if respect_retry_after {
                result.retry_after_secs.unwrap_or_else(exponential_backoff)
            } else {
                exponential_backoff()
            }
            .min(max_retry_after_s);
            attempt += 1;
            if delay_s > 0.0 {
                tokio::time::sleep(nonnegative_duration(delay_s, "HTTP retry delay")?).await;
            }
            continue;
        }

        let mut output = result.output;
        output.insert(
            format!("{}_attempts", output_key),
            serde_json::Value::Number((attempt + 1).into()),
        );
        if fail_on_status && !result.success {
            anyhow::bail!(
                "HTTP {} {} returned status {}",
                method,
                SecretEndpoint::new(&url),
                result.status
            );
        }
        return Ok(output);
    }
}

fn parse_retry_statuses(config: &serde_json::Value, ctx: &Context) -> Result<Vec<u16>> {
    let Some(values) = config
        .get("retry_statuses")
        .and_then(|value| value.as_array())
    else {
        return Ok(Vec::new());
    };
    values
        .iter()
        .map(|value| match value {
            serde_json::Value::Number(number) => number
                .as_u64()
                .and_then(|number| u16::try_from(number).ok())
                .ok_or_else(|| anyhow::anyhow!("retry_statuses values must fit in u16")),
            serde_json::Value::String(text) => interpolate_ctx(text, ctx)
                .parse::<u16>()
                .map_err(|_| anyhow::anyhow!("retry_statuses values must be HTTP status codes")),
            _ => anyhow::bail!("retry_statuses values must be numbers or numeric strings"),
        })
        .collect()
}
