use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::time::Duration;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

/// Recursively interpolate `${ctx.key}` in all string values within a JSON value.
fn interpolate_json_value(value: &serde_json::Value, ctx: &Context) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            serde_json::Value::String(interpolate_ctx(s, ctx))
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| interpolate_json_value(v, ctx)).collect())
        }
        serde_json::Value::Object(map) => {
            let new_map: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), interpolate_json_value(v, ctx)))
                .collect();
            serde_json::Value::Object(new_map)
        }
        other => other.clone(),
    }
}

async fn do_http_request(
    method: &str,
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<NodeOutput> {
    let url = config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("HTTP node requires 'url' parameter"))?;

    let url = interpolate_ctx(url, ctx);

    let timeout_s = config
        .get("timeout")
        .and_then(|v| v.as_f64())
        .unwrap_or(30.0);

    let output_key = config
        .get("output_key")
        .and_then(|v| v.as_str())
        .unwrap_or("http");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs_f64(timeout_s))
        .build()?;

    let mut request = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "PATCH" => client.patch(&url),
        _ => anyhow::bail!("Unsupported HTTP method: {}", method),
    };

    // Headers
    if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
        let mut header_map = HeaderMap::new();
        for (k, v) in headers {
            if let Some(val) = v.as_str() {
                let val = interpolate_ctx(val, ctx);
                header_map.insert(
                    HeaderName::from_bytes(k.as_bytes())?,
                    HeaderValue::from_str(&val)?,
                );
            }
        }
        request = request.headers(header_map);
    }

    // Auth
    if let Some(auth) = config.get("auth").and_then(|v| v.as_object()) {
        let auth_type = auth
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("bearer");

        match auth_type {
            "bearer" => {
                if let Some(token) = auth.get("token").and_then(|v| v.as_str()) {
                    let token = interpolate_ctx(token, ctx);
                    request = request.bearer_auth(token);
                }
            }
            "basic" => {
                let username = auth
                    .get("username")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let password = auth.get("password").and_then(|v| v.as_str());
                request = request.basic_auth(username, password);
            }
            "api_key" => {
                if let Some(key) = auth.get("key").and_then(|v| v.as_str()) {
                    let header = auth
                        .get("header")
                        .and_then(|v| v.as_str())
                        .unwrap_or("X-API-Key");
                    let key = interpolate_ctx(key, ctx);
                    request = request.header(header, key);
                }
            }
            _ => {}
        }
    }

    // Body (with recursive context interpolation)
    if let Some(body) = config.get("body") {
        let interpolated_body = interpolate_json_value(body, ctx);
        request = request.json(&interpolated_body);
    }

    let response = request.send().await?;

    let status = response.status().as_u16();
    let resp_headers: serde_json::Map<String, serde_json::Value> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                serde_json::Value::String(v.to_str().unwrap_or("").to_string()),
            )
        })
        .collect();

    let success = response.status().is_success();
    let body_text = response.text().await?;

    // Try to parse as JSON, fall back to string
    let data: serde_json::Value =
        serde_json::from_str(&body_text).unwrap_or(serde_json::Value::String(body_text));

    let mut output = NodeOutput::new();
    output.insert(
        format!("{}_status", output_key),
        serde_json::Value::Number(status.into()),
    );
    output.insert(format!("{}_data", output_key), data);
    output.insert(
        format!("{}_headers", output_key),
        serde_json::Value::Object(resp_headers),
    );
    output.insert(
        format!("{}_success", output_key),
        serde_json::Value::Bool(success),
    );

    if !success {
        anyhow::bail!("HTTP {} {} returned status {}", method, url, status);
    }

    Ok(output)
}

pub struct HttpRequestNode;

#[async_trait]
impl Node for HttpRequestNode {
    fn node_type(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Generic HTTP request with configurable method"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let method = config
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET");
        do_http_request(method, config, &ctx).await
    }
}

pub struct HttpGetNode;

#[async_trait]
impl Node for HttpGetNode {
    fn node_type(&self) -> &str {
        "http_get"
    }

    fn description(&self) -> &str {
        "HTTP GET request"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        do_http_request("GET", config, &ctx).await
    }
}

pub struct HttpPostNode;

#[async_trait]
impl Node for HttpPostNode {
    fn node_type(&self) -> &str {
        "http_post"
    }

    fn description(&self) -> &str {
        "HTTP POST request"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        do_http_request("POST", config, &ctx).await
    }
}

pub struct HttpPutNode;

#[async_trait]
impl Node for HttpPutNode {
    fn node_type(&self) -> &str {
        "http_put"
    }

    fn description(&self) -> &str {
        "HTTP PUT request"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        do_http_request("PUT", config, &ctx).await
    }
}

pub struct HttpDeleteNode;

#[async_trait]
impl Node for HttpDeleteNode {
    fn node_type(&self) -> &str {
        "http_delete"
    }

    fn description(&self) -> &str {
        "HTTP DELETE request"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        do_http_request("DELETE", config, &ctx).await
    }
}
