use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

use super::body::RequestBody;

pub(super) async fn build_request(
    client: &reqwest::Client,
    method: &str,
    url: &str,
    config: &serde_json::Value,
    ctx: &Context,
    body: &RequestBody,
    include_body: bool,
) -> Result<reqwest::RequestBuilder> {
    let request = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        _ => anyhow::bail!("Unsupported HTTP method: {}", method),
    };
    let (request, has_content_type) = apply_headers(
        request,
        config,
        ctx,
        include_body,
        include_body && body.manages_framing(),
    )?;
    let request = apply_auth(request, config, ctx);
    if include_body {
        body.apply(request, has_content_type).await
    } else {
        Ok(request)
    }
}

fn apply_headers(
    mut request: reqwest::RequestBuilder,
    config: &serde_json::Value,
    ctx: &Context,
    include_body: bool,
    managed_framing: bool,
) -> Result<(reqwest::RequestBuilder, bool)> {
    let Some(headers) = config.get("headers").and_then(|value| value.as_object()) else {
        return Ok((request, false));
    };
    let mut header_map = HeaderMap::new();
    let mut has_content_type = false;
    for (name, value) in headers {
        let normalized = name.to_ascii_lowercase();
        if managed_framing && matches!(normalized.as_str(), "content-length" | "transfer-encoding")
        {
            anyhow::bail!(
                "HTTP artifact and multipart bodies manage Content-Length and Transfer-Encoding; remove the configured '{name}' header"
            );
        }
        if !include_body
            && matches!(
                normalized.as_str(),
                "content-length" | "content-type" | "transfer-encoding"
            )
        {
            continue;
        }
        if let Some(value) = value.as_str() {
            header_map.insert(
                HeaderName::from_bytes(name.as_bytes())?,
                HeaderValue::from_str(&interpolate_ctx(value, ctx))?,
            );
            has_content_type |= name.eq_ignore_ascii_case("content-type");
        }
    }
    request = request.headers(header_map);
    Ok((request, has_content_type))
}

fn apply_auth(
    mut request: reqwest::RequestBuilder,
    config: &serde_json::Value,
    ctx: &Context,
) -> reqwest::RequestBuilder {
    let Some(auth) = config.get("auth").and_then(|value| value.as_object()) else {
        return request;
    };
    match auth
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("bearer")
    {
        "bearer" => {
            if let Some(token) = auth.get("token").and_then(|value| value.as_str()) {
                request = request.bearer_auth(interpolate_ctx(token, ctx));
            }
        }
        "basic" => {
            let username = auth
                .get("username")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let password = auth.get("password").and_then(|value| value.as_str());
            request = request.basic_auth(username, password);
        }
        "api_key" => {
            if let Some(key) = auth.get("key").and_then(|value| value.as_str()) {
                let header = auth
                    .get("header")
                    .and_then(|value| value.as_str())
                    .unwrap_or("X-API-Key");
                request = request.header(header, interpolate_ctx(key, ctx));
            }
        }
        _ => {}
    }
    request
}
