const MAX_SAME_ORIGIN_REDIRECTS: usize = 10;

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
