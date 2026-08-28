use anyhow::Result;
use std::sync::LazyLock;

use crate::engine::types::Context;
use crate::nodes::http::helpers::url_targets_internal_network;
use crate::util::sensitive_url::{SecretEndpoint, redact_sensitive_text};

use super::body::RequestBody;
use super::builder::build_request;
use super::dns::PublicDnsResolver;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProxyMode {
    Auto,
    System,
    Direct,
}

impl ProxyMode {
    pub(super) fn parse(config: &serde_json::Value) -> Result<Self> {
        match config.get("proxy_mode") {
            None => Ok(Self::Auto),
            Some(serde_json::Value::String(value)) => match value.as_str() {
                "auto" => Ok(Self::Auto),
                "system" => Ok(Self::System),
                "direct" => Ok(Self::Direct),
                _ => anyhow::bail!("HTTP proxy_mode must be 'auto', 'system', or 'direct'"),
            },
            Some(_) => anyhow::bail!("HTTP proxy_mode must be a string"),
        }
    }
}

pub(super) struct RedirectPolicy {
    pub(super) max_redirects: usize,
    pub(super) allow_cross_origin: bool,
    pub(super) block_private: bool,
    pub(super) carries_sensitive_data: bool,
}

static SYSTEM_CLIENT: LazyLock<Result<reqwest::Client, String>> =
    LazyLock::new(|| build_client(false, false));
static DIRECT_CLIENT: LazyLock<Result<reqwest::Client, String>> =
    LazyLock::new(|| build_client(true, false));
static SAFE_DIRECT_CLIENT: LazyLock<Result<reqwest::Client, String>> =
    LazyLock::new(|| build_client(true, true));

fn build_client(direct: bool, public_dns_only: bool) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .referer(false)
        .redirect(reqwest::redirect::Policy::none());
    if direct {
        builder = builder.no_proxy();
    }
    if public_dns_only {
        builder = builder.dns_resolver(PublicDnsResolver);
    }
    builder.build().map_err(|error| error.to_string())
}

pub(super) fn shared_client(proxy_mode: ProxyMode, block_private: bool) -> Result<reqwest::Client> {
    let client = if block_private {
        if proxy_mode == ProxyMode::System {
            anyhow::bail!(
                "HTTP proxy_mode='system' cannot be combined with block_private_network=true; use 'auto' or 'direct'"
            );
        }
        &*SAFE_DIRECT_CLIENT
    } else if proxy_mode == ProxyMode::Direct {
        &*DIRECT_CLIENT
    } else {
        &*SYSTEM_CLIENT
    };
    client.as_ref().cloned().map_err(|error| {
        anyhow::anyhow!(
            "Failed to build HTTP client: {}",
            redact_sensitive_text(error)
        )
    })
}

pub(super) async fn send_with_redirects(
    client: &reqwest::Client,
    method: &str,
    url: &str,
    config: &serde_json::Value,
    ctx: &Context,
    body: &RequestBody,
    policy: &RedirectPolicy,
) -> Result<reqwest::Response> {
    send_chain(client, method, url, config, ctx, body, policy).await
}

async fn send_chain(
    client: &reqwest::Client,
    method: &str,
    url: &str,
    config: &serde_json::Value,
    ctx: &Context,
    body: &RequestBody,
    policy: &RedirectPolicy,
) -> Result<reqwest::Response> {
    let mut current_url = parse_url(url)?;
    let mut current_method = method.to_uppercase();
    let mut include_body = true;
    let mut redirects = 0_usize;

    loop {
        enforce_literal_network_policy(&current_url, policy.block_private)?;
        let response = build_request(
            client,
            &current_method,
            current_url.as_str(),
            config,
            ctx,
            body,
            include_body,
        )
        .await?
        .send()
        .await
        .map_err(|error| transport_error(method, url, error))?;

        if !response.status().is_redirection() {
            return Ok(response);
        }
        if policy.max_redirects == 0 {
            return Ok(response);
        }
        let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
            return Ok(response);
        };
        if redirects >= policy.max_redirects {
            anyhow::bail!(
                "HTTP {} request to {} failed: too many redirects",
                method,
                SecretEndpoint::new(url)
            );
        }
        let location = location
            .to_str()
            .map_err(|_| anyhow::anyhow!("HTTP redirect Location header is not valid ASCII"))?;
        let target = current_url
            .join(location)
            .map_err(|_| anyhow::anyhow!("HTTP redirect Location is not a valid URL"))?;
        validate_redirect(&current_url, &target, policy)?;

        let (next_method, next_has_body) = redirected_method(response.status(), &current_method);
        current_method = next_method;
        include_body = include_body && next_has_body;
        current_url = target;
        redirects += 1;
    }
}

fn parse_url(url: &str) -> Result<url::Url> {
    let parsed = url::Url::parse(url)
        .map_err(|_| anyhow::anyhow!("HTTP request URL is not a valid absolute URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("HTTP request URL scheme must be http or https");
    }
    Ok(parsed)
}

fn validate_redirect(current: &url::Url, target: &url::Url, policy: &RedirectPolicy) -> Result<()> {
    if !matches!(target.scheme(), "http" | "https") {
        anyhow::bail!("HTTP redirect target scheme must be http or https");
    }
    if !target.username().is_empty() || target.password().is_some() {
        anyhow::bail!("HTTP redirect target cannot contain URL credentials");
    }
    enforce_literal_network_policy(target, policy.block_private)?;

    let cross_origin = current.scheme() != target.scheme()
        || current.host() != target.host()
        || current.port_or_known_default() != target.port_or_known_default();
    if cross_origin && policy.carries_sensitive_data {
        anyhow::bail!(
            "cross-origin redirect refused because the request carries configured auth, headers, or a body, including URL credentials"
        );
    }
    if cross_origin && !policy.allow_cross_origin {
        anyhow::bail!(
            "cross-origin redirects are disabled; set allow_cross_origin_redirects=true for requests without configured auth, headers, or a body"
        );
    }
    Ok(())
}

fn enforce_literal_network_policy(url: &url::Url, block_private: bool) -> Result<()> {
    if block_private && url_targets_internal_network(url) {
        anyhow::bail!(
            "HTTP request to {} blocked: target is a private network address (block_private_network is enabled)",
            SecretEndpoint::new(url.as_str())
        );
    }
    Ok(())
}

fn redirected_method(status: reqwest::StatusCode, method: &str) -> (String, bool) {
    let switch_to_get = status == reqwest::StatusCode::SEE_OTHER && method != "HEAD"
        || matches!(
            status,
            reqwest::StatusCode::MOVED_PERMANENTLY | reqwest::StatusCode::FOUND
        ) && method == "POST";
    if switch_to_get {
        ("GET".to_owned(), false)
    } else {
        (method.to_owned(), true)
    }
}

fn transport_error(method: &str, url: &str, error: reqwest::Error) -> anyhow::Error {
    let detail = format!("{:#}", anyhow::Error::new(error));
    anyhow::anyhow!(
        "HTTP {} request to {} failed: {}",
        method,
        SecretEndpoint::new(url),
        redact_sensitive_text(&detail)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_mode_is_strict() {
        assert_eq!(
            ProxyMode::parse(&serde_json::json!({})).unwrap(),
            ProxyMode::Auto
        );
        assert_eq!(
            ProxyMode::parse(&serde_json::json!({"proxy_mode": "direct"})).unwrap(),
            ProxyMode::Direct
        );
        assert!(ProxyMode::parse(&serde_json::json!({"proxy_mode": false})).is_err());
        assert!(ProxyMode::parse(&serde_json::json!({"proxy_mode": "other"})).is_err());
    }

    #[test]
    fn safe_network_policy_refuses_explicit_system_proxy() {
        let error = shared_client(ProxyMode::System, true)
            .unwrap_err()
            .to_string();
        assert!(error.contains("cannot be combined"), "{error}");
    }

    #[test]
    fn redirect_method_matches_common_http_semantics() {
        assert_eq!(
            redirected_method(reqwest::StatusCode::FOUND, "POST"),
            ("GET".to_owned(), false)
        );
        assert_eq!(
            redirected_method(reqwest::StatusCode::TEMPORARY_REDIRECT, "POST"),
            ("POST".to_owned(), true)
        );
        assert_eq!(
            redirected_method(reqwest::StatusCode::SEE_OTHER, "HEAD"),
            ("HEAD".to_owned(), true)
        );
    }
}
