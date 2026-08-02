mod builder;
mod nodes;
mod response;

use anyhow::Result;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::util::duration::{nonnegative_duration, positive_duration};
use crate::util::node_config::{
    config_bool_or, config_f64_or, config_u64_strict, config_usize_strict,
};
use crate::util::sensitive_url::{SecretEndpoint, redact_sensitive_text};

use builder::build_request;
pub use nodes::{HttpDeleteNode, HttpGetNode, HttpPostNode, HttpPutNode, HttpRequestNode};
use response::response_to_output;

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

    let timeout_s = config_f64_or(config, "timeout", ctx, 30.0)?;
    let timeout = positive_duration(timeout_s, "HTTP timeout")?;
    let output_key = config
        .get("output_key")
        .and_then(|value| value.as_str())
        .unwrap_or("http");
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
    let carries_redirect_sensitive_data = carries_redirect_sensitive_data(config, &url);
    // This is a security control: a typo must fail closed rather than silently
    // turning private-network protection off.
    let block_private = config_bool_or(config, "block_private_network", ctx, false)?;

    if block_private
        && let Ok(parsed) = url::Url::parse(&url)
        && crate::nodes::http::helpers::url_targets_internal_network(&parsed)
    {
        anyhow::bail!(
            "HTTP request to {} blocked: target is a private network address (block_private_network is enabled)",
            SecretEndpoint::new(&url)
        );
    }

    let redirect_policy = if max_redirects == 0 {
        reqwest::redirect::Policy::none()
    } else {
        reqwest::redirect::Policy::custom(move |attempt| {
            // Reqwest includes the initial URL in `previous`, so `>` permits
            // exactly `max_redirects` hops while still bounding the chain.
            if attempt.previous().len() > max_redirects {
                attempt.error("too many redirects")
            } else if block_private
                && crate::nodes::http::helpers::url_targets_internal_network(attempt.url())
            {
                attempt.error("redirect to a private network address is blocked")
            } else if redirect_changes_origin(&attempt) && carries_redirect_sensitive_data {
                // Reqwest strips a small fixed set (such as Authorization),
                // but forwards arbitrary headers including X-API-Key. It also
                // cannot identify secrets in user-named headers, and URL
                // userinfo becomes an Authorization header only while reqwest
                // builds the request. Do not let an opt-in cross-origin policy
                // override this hard fence.
                attempt.error(
                    "cross-origin redirect refused because the request carries configured auth, headers, or a body, including URL credentials",
                )
            } else if redirect_changes_origin(&attempt) && !allow_cross_origin_redirects {
                attempt.error(
                    "cross-origin redirects are disabled; set allow_cross_origin_redirects=true for requests without configured auth, headers, or a body",
                )
            } else {
                attempt.follow()
            }
        })
    };

    let client = reqwest::Client::builder()
        .timeout(timeout)
        // Reqwest's generated cross-origin Referer retains the original query
        // string. URLs commonly carry signed tokens, so never synthesize one.
        .referer(false)
        .redirect(redirect_policy)
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
                // Reqwest's Display text stops at "error following redirect";
                // preserve the custom policy's safe reason from the source
                // chain so operators can distinguish a security refusal from a
                // transport failure. The whole chain still passes through the
                // URL/credential scrubber before it leaves the node.
                let detail = format!("{:#}", anyhow::Error::new(error));
                anyhow::anyhow!(
                    "HTTP {} request to {} failed: {}",
                    method,
                    SecretEndpoint::new(&url),
                    redact_sensitive_text(&detail)
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
            .min(max_retry_after_s)
            .max(MIN_STATUS_RETRY_DELAY_SECONDS);
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

fn redirect_changes_origin(attempt: &reqwest::redirect::Attempt<'_>) -> bool {
    let Some(previous) = attempt.previous().last() else {
        return true;
    };
    previous.scheme() != attempt.url().scheme()
        || previous.host() != attempt.url().host()
        || previous.port_or_known_default() != attempt.url().port_or_known_default()
}

fn carries_redirect_sensitive_data(config: &serde_json::Value, request_url: &str) -> bool {
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
    has_auth || has_headers || has_url_credentials || config.get("body").is_some()
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
