use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::http::helpers::{body_value_to_text, build_form_body, interpolate_json_value};

pub(super) fn build_request(
    client: &reqwest::Client,
    method: &str,
    url: &str,
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<reqwest::RequestBuilder> {
    let request = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        _ => anyhow::bail!("Unsupported HTTP method: {}", method),
    };
    let (request, has_content_type) = apply_headers(request, config, ctx)?;
    let request = apply_auth(request, config, ctx);
    apply_body(request, config, ctx, has_content_type)
}

fn apply_headers(
    mut request: reqwest::RequestBuilder,
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<(reqwest::RequestBuilder, bool)> {
    let Some(headers) = config.get("headers").and_then(|value| value.as_object()) else {
        return Ok((request, false));
    };
    let mut header_map = HeaderMap::new();
    let mut has_content_type = false;
    for (name, value) in headers {
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

fn apply_body(
    mut request: reqwest::RequestBuilder,
    config: &serde_json::Value,
    ctx: &Context,
    has_content_type: bool,
) -> Result<reqwest::RequestBuilder> {
    let Some(body) = config.get("body") else {
        return Ok(request);
    };
    let body = interpolate_json_value(body, ctx);
    match config
        .get("body_type")
        .and_then(|value| value.as_str())
        .unwrap_or("json")
    {
        "json" => request = request.json(&body),
        "form" => {
            if !has_content_type {
                request = request.header("Content-Type", "application/x-www-form-urlencoded");
            }
            request = request.body(build_form_body(&body)?);
        }
        "text" => {
            if !has_content_type {
                request = request.header("Content-Type", "text/plain; charset=utf-8");
            }
            request = request.body(body_value_to_text(&body));
        }
        other => anyhow::bail!(
            "Unsupported body_type '{}'. Expected one of: json, form, text",
            other
        ),
    }
    Ok(request)
}
