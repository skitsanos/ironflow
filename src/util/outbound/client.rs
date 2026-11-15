//! Installing the guard on a reqwest client.

use super::Guard;

/// The proxy host a guarded client is pointed at for a destination the first
/// stage refuses. It never resolves: the resolver below fails it, so the
/// request ends before any connection.
pub(super) const BLOCKED_HOST: &str = "private-network-blocked.invalid";

impl Guard {
    /// Make `builder` enforce this guard on every connection: no proxy (a
    /// proxy would resolve the destination itself), the pinning resolver for
    /// host names, and a refusal for every request whose literal destination
    /// fails the first stage. That last part needs no cooperation from the
    /// call site: reqwest asks this hook about each request URL, and a refused
    /// one is sent to a proxy host that the resolver never resolves.
    pub(crate) fn install(self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        let resolver = self.dns_resolver();
        builder
            .no_proxy()
            .proxy(reqwest::Proxy::custom(move |url| {
                self.check_url(url).is_err().then(blocked_proxy)
            }))
            .dns_resolver(resolver)
    }
}

#[allow(clippy::expect_used)] // Infallible: a constant, valid URL.
fn blocked_proxy() -> reqwest::Url {
    reqwest::Url::parse(&format!("http://{BLOCKED_HOST}:9/")).expect("constant URL")
}
