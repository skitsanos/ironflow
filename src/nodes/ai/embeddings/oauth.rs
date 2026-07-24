use std::sync::LazyLock;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Deserialize;

use crate::util::bounded_cache::BoundedCache;

#[derive(Clone)]
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

type OauthKey = (String, String, Option<String>);

/// Hard upper bound on distinct OAuth client tuples remembered across the process.
/// Override with `IRONFLOW_OAUTH_CACHE_SIZE`.
const DEFAULT_OAUTH_CACHE_SIZE: usize = 128;

fn oauth_cache_capacity() -> usize {
    std::env::var("IRONFLOW_OAUTH_CACHE_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|capacity| *capacity > 0)
        .unwrap_or(DEFAULT_OAUTH_CACHE_SIZE)
}

static OAUTH_TOKEN_CACHE: LazyLock<BoundedCache<OauthKey, CachedToken>> =
    LazyLock::new(|| BoundedCache::new(oauth_cache_capacity()));

fn percent_encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => result.push_str(&format!("%{:02X}", byte)),
        }
    }
    result
}

pub(in crate::nodes::ai) async fn acquire_oauth_token(
    client: &reqwest::Client,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    scope: Option<&str>,
) -> Result<String> {
    let cache_key = (
        token_url.to_string(),
        client_id.to_string(),
        scope.map(str::to_string),
    );

    if let Some(cached) = OAUTH_TOKEN_CACHE.get(&cache_key)
        && Instant::now() + Duration::from_secs(60) < cached.expires_at
    {
        return Ok(cached.access_token);
    }

    let mut form_body = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}",
        percent_encode(client_id),
        percent_encode(client_secret),
    );
    if let Some(scope) = scope {
        form_body.push_str(&format!("&scope={}", percent_encode(scope)));
    }

    let response = client
        .post(token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("OAuth token request failed: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| anyhow::anyhow!("Failed to read OAuth token response: {}", error))?;

    if !status.is_success() {
        anyhow::bail!("OAuth token request failed ({}): {}", status, body);
    }

    let token: TokenResponse = serde_json::from_str(&body)
        .map_err(|error| anyhow::anyhow!("Failed to parse OAuth token response: {}", error))?;
    let access_token = token.access_token.clone();
    OAUTH_TOKEN_CACHE.insert(
        cache_key,
        CachedToken {
            access_token: token.access_token,
            expires_at: Instant::now() + Duration::from_secs(token.expires_in),
        },
        None,
    );

    Ok(access_token)
}

#[cfg(test)]
mod tests {
    use super::percent_encode;

    #[test]
    fn percent_encode_uses_form_safe_bytes() {
        assert_eq!(
            percent_encode("client + secret/\u{00e9}"),
            "client%20%2B%20secret%2F%C3%A9"
        );
    }
}
