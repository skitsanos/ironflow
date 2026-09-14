const MAX_SAME_ORIGIN_REDIRECTS: usize = 10;

/// Admit notification response bytes before decoding either success or error text.
pub(crate) async fn notification_response_text(
    mut response: reqwest::Response,
) -> anyhow::Result<String> {
    let maximum = super::limits::max_http_body_bytes();
    if let Some(length) = response.content_length()
        && length > maximum
    {
        anyhow::bail!(
            "response content-length {length} exceeds IRONFLOW_MAX_HTTP_BODY_BYTES ({maximum})"
        );
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .cloned();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        anyhow::anyhow!(
            "{}",
            super::sensitive_url::redact_sensitive_text(&error.to_string())
        )
    })? {
        if chunk.len() as u64 > maximum.saturating_sub(bytes.len() as u64) {
            anyhow::bail!(
                "response body exceeds IRONFLOW_MAX_HTTP_BODY_BYTES ({maximum}) while streaming"
            );
        }
        bytes
            .try_reserve_exact(chunk.len())
            .map_err(|_| anyhow::anyhow!("cannot reserve memory for notification response"))?;
        bytes.extend_from_slice(&chunk);
        // A stream of immediately ready chunks must still let the executor cancel.
        tokio::task::yield_now().await;
    }

    // Decode only the admitted in-memory body through Reqwest to retain charset,
    // BOM, and replacement-character behavior without another network read.
    let mut bounded = http::Response::new(bytes);
    if let Some(content_type) = content_type {
        bounded
            .headers_mut()
            .insert(reqwest::header::CONTENT_TYPE, content_type);
    }
    reqwest::Response::from(bounded)
        .text()
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "Failed to decode notification response: {}",
                super::sensitive_url::redact_sensitive_text(&error.to_string())
            )
        })
}

/// Provider requests carry credentials or private payloads, often both.
pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() > MAX_SAME_ORIGIN_REDIRECTS {
                return attempt.error("too many provider redirects");
            }
            let Some(origin) = attempt.previous().first() else {
                return attempt.error("provider redirect has no source origin");
            };
            if same_origin(origin, attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("cross-origin provider redirect refused")
            }
        }))
        .referer(false)
}

fn same_origin(source: &reqwest::Url, target: &reqwest::Url) -> bool {
    source.scheme() == target.scheme()
        && source.host() == target.host()
        && source.port_or_known_default() == target.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::same_origin;

    #[test]
    fn origin_includes_scheme_host_and_effective_port() {
        let source = reqwest::Url::parse("https://provider.example/private?key=fixture").unwrap();
        for (target, expected) in [
            ("https://provider.example:443/next", true),
            ("https://PROVIDER.example/next", true),
            ("http://provider.example/next", false),
            ("https://provider.example:444/next", false),
            ("https://other.example/next", false),
        ] {
            assert_eq!(
                same_origin(&source, &reqwest::Url::parse(target).unwrap()),
                expected
            );
        }
    }
}
