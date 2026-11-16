use std::sync::LazyLock;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::util::bounded_cache::BoundedCache;

#[derive(Clone)]
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    /// Optional in RFC 6749; some providers send the number as a string.
    #[serde(default)]
    expires_in: Option<serde_json::Value>,
}

/// The token lifetime in whole seconds.
///
/// `Ok(None)` means the provider gave none, so the token is used once and not
/// cached. A value that is present but not a non-negative number is an error.
fn token_lifetime(value: Option<&serde_json::Value>) -> Result<Option<u64>, ()> {
    let seconds = match value {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(serde_json::Value::Number(number)) => number.as_f64(),
        Some(serde_json::Value::String(text)) => text.trim().parse::<f64>().ok(),
        Some(_) => None,
    };
    match seconds {
        // The cast saturates, and `token_expiry` checks the addition.
        Some(seconds) if seconds.is_finite() && seconds >= 0.0 => Ok(Some(seconds as u64)),
        _ => Err(()),
    }
}

/// Token URL, client ID, scope and a digest of the client secret.
///
/// The secret is part of the identity: without it, a caller that knows only a
/// client ID would be handed the token another caller authenticated for.
type OauthKey = (String, String, Option<String>, [u8; 32]);

/// Hard upper bound on distinct OAuth client tuples remembered across the
/// process; `IRONFLOW_OAUTH_CACHE_SIZE` overrides it (see `util::limits`).
fn oauth_cache_capacity() -> usize {
    crate::util::limits::oauth_cache_size()
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

/// One OAuth client-credentials identity.
pub(in crate::nodes::ai) struct OauthClient<'a> {
    pub token_url: &'a str,
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub scope: Option<&'a str>,
}

impl OauthClient<'_> {
    fn cache_key(&self) -> OauthKey {
        (
            self.token_url.to_string(),
            self.client_id.to_string(),
            self.scope.map(str::to_string),
            Sha256::digest(self.client_secret.as_bytes()).into(),
        )
    }
}

pub(in crate::nodes::ai) async fn acquire_oauth_token(
    client: &reqwest::Client,
    oauth: &OauthClient<'_>,
) -> Result<String> {
    let cache_key = oauth.cache_key();

    if let Some(cached) = OAUTH_TOKEN_CACHE.get(&cache_key)
        && Instant::now() + Duration::from_secs(60) < cached.expires_at
    {
        return Ok(cached.access_token);
    }

    let mut form_body = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}",
        percent_encode(oauth.client_id),
        percent_encode(oauth.client_secret),
    );
    if let Some(scope) = oauth.scope {
        form_body.push_str(&format!("&scope={}", percent_encode(scope)));
    }

    crate::util::outbound::ensure_url_allowed("embedding OAuth", oauth.token_url).await?;
    // The token URL, the form and the provider's reply can all carry
    // credentials, so errors name only the kind of failure.
    let response = client
        .post(oauth.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .map_err(|error| {
            let kind = if error.is_redirect() {
                "redirect refused"
            } else if error.is_timeout() {
                "request timed out"
            } else if error.is_connect() {
                "connection failed"
            } else {
                "request failed"
            };
            anyhow::anyhow!("OAuth token {kind}")
        })?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("OAuth token request failed (HTTP {status}); provider body omitted");
    }
    let body = crate::util::provider_http::bounded_response_text(response)
        .await
        .map_err(|error| anyhow::anyhow!("OAuth token response rejected: {error}"))?;

    let (access_token, lifetime) = serde_json::from_str::<TokenResponse>(&body)
        .ok()
        .filter(|token| !token.access_token.is_empty())
        .and_then(|token| {
            let lifetime = token_lifetime(token.expires_in.as_ref()).ok()?;
            Some((token.access_token, lifetime))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "OAuth token response is not a token document with a non-empty \
                 'access_token' string and, when present, a non-negative numeric \
                 'expires_in'; provider body omitted"
            )
        })?;
    // Without a stated lifetime the token is used for this call only.
    if let Some(lifetime) = lifetime {
        OAUTH_TOKEN_CACHE.insert(
            cache_key,
            CachedToken {
                access_token: access_token.clone(),
                expires_at: token_expiry(Instant::now(), lifetime),
            },
            None,
        );
    }

    Ok(access_token)
}

/// Forgets `rejected` after the provider answered 401 for it.
///
/// A token another task has already replaced stays cached.
pub(in crate::nodes::ai) fn evict_oauth_token(oauth: &OauthClient<'_>, rejected: &str) {
    let cache_key = oauth.cache_key();
    if OAUTH_TOKEN_CACHE
        .get(&cache_key)
        .is_some_and(|cached| cached.access_token == rejected)
    {
        OAUTH_TOKEN_CACHE.remove(&cache_key);
    }
}

/// When a token fetched at `now` expires.
///
/// `expires_in` comes from the provider's response. `Instant + Duration`
/// panics on overflow, so a lifetime too large to represent expires at once:
/// the token is still returned for this call and fetched again on the next.
fn token_expiry(now: Instant, expires_in: u64) -> Instant {
    now.checked_add(Duration::from_secs(expires_in))
        .unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use serde_json::json;

    use super::{percent_encode, token_expiry, token_lifetime};

    #[test]
    fn token_expiry_does_not_overflow_on_a_hostile_lifetime() {
        let now = Instant::now();
        assert_eq!(token_expiry(now, 3600), now + Duration::from_secs(3600));
        assert_eq!(token_expiry(now, u64::MAX), now);
    }

    #[test]
    fn token_lifetime_accepts_numbers_and_numeric_strings_only() {
        for (value, expected) in [
            (json!(3600), Ok(Some(3600))),
            (json!(3600.9), Ok(Some(3600))),
            (json!(" 3600 "), Ok(Some(3600))),
            (json!(0), Ok(Some(0))),
            (json!(1e300), Ok(Some(u64::MAX))),
            (json!(null), Ok(None)),
            (json!(-1), Err(())),
            (json!("NaN"), Err(())),
            (json!("inf"), Err(())),
            (json!(""), Err(())),
            (json!({"seconds": 1}), Err(())),
        ] {
            assert_eq!(token_lifetime(Some(&value)), expected, "{value}");
        }
        assert_eq!(token_lifetime(None), Ok(None));
    }

    #[test]
    fn percent_encode_uses_form_safe_bytes() {
        assert_eq!(
            percent_encode("client + secret/\u{00e9}"),
            "client%20%2B%20secret%2F%C3%A9"
        );
    }
}
