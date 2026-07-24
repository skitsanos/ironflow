use anyhow::Result;

use crate::engine::types::NodeOutput;
use crate::util::sensitive_url::redact_sensitive_text;

pub(super) struct HttpResponseOutput {
    pub(super) status: u16,
    pub(super) success: bool,
    pub(super) output: NodeOutput,
    pub(super) retry_after_secs: Option<f64>,
}

pub(super) async fn response_to_output(
    mut response: reqwest::Response,
    output_key: &str,
) -> Result<HttpResponseOutput> {
    let status = response.status().as_u16();
    let success = response.status().is_success();
    let retry_after_secs = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value >= 0.0);
    let response_headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                serde_json::Value::String(value.to_str().unwrap_or("").to_string()),
            )
        })
        .collect();

    let max_body = crate::util::limits::max_http_body_bytes();
    if let Some(length) = response.content_length()
        && length > max_body
    {
        anyhow::bail!(
            "HTTP response body {} bytes exceeds limit {} (set IRONFLOW_MAX_HTTP_BODY_BYTES to raise)",
            length,
            max_body
        );
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        anyhow::anyhow!(
            "Failed to read HTTP response: {}",
            redact_sensitive_text(&error.to_string())
        )
    })? {
        if bytes.len() as u64 + chunk.len() as u64 > max_body {
            anyhow::bail!(
                "HTTP response body exceeds limit {} bytes mid-stream (set IRONFLOW_MAX_HTTP_BODY_BYTES to raise)",
                max_body
            );
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&bytes).into_owned();
    let data = serde_json::from_str(&body).unwrap_or(serde_json::Value::String(body));

    let mut output = NodeOutput::new();
    output.insert(
        format!("{}_status", output_key),
        serde_json::Value::Number(status.into()),
    );
    output.insert(format!("{}_data", output_key), data);
    output.insert(
        format!("{}_headers", output_key),
        serde_json::Value::Object(response_headers),
    );
    output.insert(
        format!("{}_success", output_key),
        serde_json::Value::Bool(success),
    );

    Ok(HttpResponseOutput {
        status,
        success,
        output,
        retry_after_secs,
    })
}
