//! The guarded HTTPS client for AWS SDK service clients (S3, S3 Vectors).

use aws_smithy_http_client::{Builder, tls};
use aws_smithy_runtime_api::client::dns::{DnsFuture, ResolveDns, ResolveDnsError};
use aws_smithy_runtime_api::client::http::SharedHttpClient;

use super::Guard;
use super::allow::normalize_name;

/// The SDK's default HTTPS client with the guard's resolver: direct
/// connections, and every DNS answer validated before it is connected to.
/// It is set on the service client only, so the SDK credential providers
/// keep their own client and can still reach an instance metadata service.
pub(crate) fn http_client(guard: Guard, endpoint_url: Option<&str>) -> SharedHttpClient {
    let endpoint_host = endpoint_url
        .and_then(|endpoint| url::Url::parse(endpoint).ok())
        .and_then(|endpoint| endpoint.host_str().map(normalize_name));
    Builder::new()
        .tls_provider(tls::Provider::Rustls(
            tls::rustls_provider::CryptoMode::AwsLc,
        ))
        .build_with_resolver(GuardedAwsResolver {
            guard,
            endpoint_host,
        })
}

#[derive(Clone, Debug)]
struct GuardedAwsResolver {
    guard: Guard,
    endpoint_host: Option<String>,
}

impl GuardedAwsResolver {
    /// A virtual-hosted bucket is addressed as `<bucket>.<endpoint host>`.
    /// It is judged as the configured endpoint host, so an allowlist entry
    /// for that host also covers its buckets.
    fn judged_name<'a>(&'a self, name: &'a str) -> &'a str {
        match &self.endpoint_host {
            Some(host)
                if normalize_name(name)
                    .strip_suffix(host.as_str())
                    .is_some_and(|bucket| bucket.ends_with('.')) =>
            {
                host
            }
            _ => name,
        }
    }
}

impl ResolveDns for GuardedAwsResolver {
    fn resolve_dns<'a>(&'a self, name: &'a str) -> DnsFuture<'a> {
        DnsFuture::new(async move {
            let addresses = self
                .guard
                .lookup(name, 0)
                .await
                .map_err(ResolveDnsError::new)?;
            let validated = self
                .guard
                .validate_answer(self.judged_name(name), addresses)
                .map_err(ResolveDnsError::new)?;
            Ok(validated.into_iter().map(|address| address.ip()).collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;
    use crate::util::outbound::test_scope::with_policy;

    const HOSTS: &[(&str, &[&str])] = &[
        ("internal.test", &["10.0.0.5"]),
        ("minio.test", &["10.0.0.6"]),
        ("bucket.minio.test", &["10.0.0.7"]),
        ("bucket.minio.test.evil.test", &["10.0.0.8"]),
        ("notminio.test", &["10.0.0.9"]),
        ("public.test", &["93.184.216.34"]),
    ];

    async fn resolver(allowlist: &[&str], endpoint: Option<&str>) -> GuardedAwsResolver {
        let guard = with_policy(true, allowlist, HOSTS, async {
            Guard::resolve(false).unwrap().unwrap()
        })
        .await;
        GuardedAwsResolver {
            guard,
            endpoint_host: endpoint
                .and_then(|endpoint| url::Url::parse(endpoint).ok())
                .and_then(|endpoint| endpoint.host_str().map(normalize_name)),
        }
    }

    #[tokio::test]
    async fn dns_answers_are_validated_for_the_sdk_client() {
        let resolver = resolver(&[], Some("http://internal.test:9000")).await;
        for name in ["internal.test", "minio.test", "bucket.minio.test"] {
            let error = resolver.resolve_dns(name).await.unwrap_err();
            let source = std::error::Error::source(&error).unwrap().to_string();
            assert!(source.contains("private network"), "{name}: {source}");
        }
        let public: Vec<IpAddr> = resolver.resolve_dns("public.test").await.unwrap();
        assert_eq!(public, vec!["93.184.216.34".parse::<IpAddr>().unwrap()]);
    }

    #[tokio::test]
    async fn an_allowlisted_endpoint_covers_its_virtual_hosted_buckets_only() {
        let resolver = resolver(&["minio.test:9000"], Some("http://MINIO.test:9000")).await;
        for name in ["minio.test", "bucket.minio.test", "Bucket.Minio.Test."] {
            assert!(resolver.resolve_dns(name).await.is_ok(), "{name}");
        }
        for name in [
            "notminio.test",
            "bucket.minio.test.evil.test",
            "internal.test",
        ] {
            assert!(resolver.resolve_dns(name).await.is_err(), "{name}");
        }
    }
}
